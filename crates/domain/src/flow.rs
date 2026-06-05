//! Orchestration: drive one `ClassificationInput` through the prompt loop,
//! emit events eagerly via `write_event`, and return a `ProcessActionResult`
//! (the subset of `ClassificationRecord` fields the prompt flow populates).
//! Composition roots merge the outcome with identity / pid / duration slices
//! and call `history.write(record)` once at the end.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::{
    Agent, AgentId, AgentStore, ClassificationEvent, ClassificationHistoryWriter,
    ClassificationInput, Classifier, ClassifierOutcome, EventKind, LlmClassifier, OverrideAction,
    PromptError, PromptLock, Prompter, PromptingDeps, RecordedClassifierOutcome, RedKind, Scope,
    ScopeOrigin, ScopeStoreError, StandingGuidance, StandingGuidanceReader, Verdict, VerdictChoice,
};

pub fn resolve_scope<S: AgentStore>(
    store: &S,
    agent: &Agent,
) -> Result<Option<Scope>, ScopeStoreError> {
    store.scope_for(agent.id())
}

/// Subagent + root inheritance key for the disabled-flag check. Subagents
/// inherit their parent's disabled state at depth-1 (full chain walk is
/// future work; reconstructable at query time today).
fn root_key(agent: &Agent) -> &AgentId {
    match agent {
        Agent::Root { id } => id,
        Agent::Subagent { parent_id, .. } => parent_id,
    }
}

/// Slice of [`crate::ClassificationRecord`] populated by `process_action`.
/// Composition root merges with identity, pid, hook duration, and the result
/// of [`crate::resolve_enforcement`] to build the full record.
pub struct ProcessActionResult {
    pub scope: Option<Scope>,
    pub scope_origin: Option<ScopeOrigin>,
    pub llm_outcome: Option<RecordedClassifierOutcome>,
    pub prompt_shown: bool,
    pub prompt_latency: Option<Duration>,
    pub lock_wait: Option<Duration>,
    pub initial_verdict: Option<Verdict>,
    pub verdict: Verdict,
    pub override_action: Option<OverrideAction>,
    pub error_chain: Option<String>,
}

impl ProcessActionResult {
    fn red(kind: RedKind, reason: String) -> Self {
        Self {
            scope: None,
            scope_origin: None,
            llm_outcome: None,
            prompt_shown: false,
            prompt_latency: None,
            lock_wait: None,
            initial_verdict: None,
            verdict: Verdict::Red { kind, reason },
            override_action: None,
            error_chain: None,
        }
    }
}

#[allow(clippy::too_many_arguments)] // composition glue: each port is a distinct collaborator
pub fn process_action<S, H, C, P, K, G>(
    input: &ClassificationInput,
    store: &S,
    classifier: &Classifier<C>,
    history: &H,
    prompting: &PromptingDeps<'_, P, K>,
    guidance_reader: &G,
    dev_bypass: bool,
) -> ProcessActionResult
where
    S: AgentStore,
    H: ClassificationHistoryWriter,
    C: LlmClassifier,
    P: Prompter,
    K: PromptLock,
    G: StandingGuidanceReader,
{
    match guidance_reader.read() {
        Ok(guidance) => process_inner(
            input, store, classifier, history, prompting, &guidance, dev_bypass,
        ),
        Err(e) => {
            let chain = join_source_chain(&e);
            let _ = history.write_event(transition(
                input,
                format!("standing-guidance-error: {chain}"),
                dev_bypass,
            ));
            let mut outcome = ProcessActionResult::red(
                RedKind::StandingGuidanceUnavailable,
                format!("standing guidance unavailable: {e}"),
            );
            outcome.error_chain = Some(chain);
            outcome
        }
    }
}

/// `Mutated` invalidates the classify cache (we wrote new state); `Retry`
/// re-loops with cache intact (cheap re-prompt after a cancel).
enum Step {
    Done {
        verdict: Verdict,
        override_action: Option<OverrideAction>,
    },
    Mutated {
        new_scope_origin: Option<ScopeOrigin>,
    },
    Retry,
}

#[allow(clippy::too_many_arguments)] // each port is a distinct collaborator
fn process_inner<S, H, C, P, K>(
    input: &ClassificationInput,
    store: &S,
    classifier: &Classifier<C>,
    history: &H,
    prompting: &PromptingDeps<'_, P, K>,
    standing: &StandingGuidance,
    dev_bypass: bool,
) -> ProcessActionResult
where
    S: AgentStore,
    H: ClassificationHistoryWriter,
    C: LlmClassifier,
    P: Prompter,
    K: PromptLock,
{
    let key = root_key(&input.agent).clone();
    let mut lock_guard: Option<K::Guard> = None;
    let mut lock_wait: Option<Duration> = None;
    let mut prompt_latency: Option<Duration> = None;
    let mut cached: Option<(Option<Scope>, ClassifierOutcome)> = None;
    let mut initial_verdict: Option<Verdict> = None;
    let mut last_outcome: Option<ClassifierOutcome> = None;
    let mut prompt_shown = false;
    let mut scope_origin: Option<ScopeOrigin> = None;

    loop {
        match store.is_disabled(&key) {
            Ok(true) => {
                let msg = if lock_guard.is_some() {
                    "disabled-during-wait"
                } else {
                    "session-disabled"
                };
                let _ = history.write_event(transition(input, msg.into(), dev_bypass));
                return ProcessActionResult {
                    scope: cached.as_ref().and_then(|(s, _)| s.clone()),
                    scope_origin,
                    llm_outcome: last_outcome.as_ref().map(RecordedClassifierOutcome::from),
                    prompt_shown,
                    prompt_latency,
                    lock_wait,
                    initial_verdict,
                    verdict: Verdict::Green,
                    override_action: Some(OverrideAction::DisableSession),
                    error_chain: None,
                };
            }
            Ok(false) => {}
            Err(e) => {
                return store_error_outcome(
                    input,
                    history,
                    &e,
                    "is_disabled",
                    dev_bypass,
                    cached.as_ref().and_then(|(s, _)| s.clone()),
                    scope_origin,
                    initial_verdict,
                    last_outcome.as_ref().map(RecordedClassifierOutcome::from),
                    prompt_shown,
                    prompt_latency,
                    lock_wait,
                );
            }
        }

        let scope = match resolve_scope(store, &input.agent) {
            Ok(s) => s,
            Err(e) => {
                return store_error_outcome(
                    input,
                    history,
                    &e,
                    "scope_for",
                    dev_bypass,
                    None,
                    scope_origin,
                    initial_verdict,
                    last_outcome.as_ref().map(RecordedClassifierOutcome::from),
                    prompt_shown,
                    prompt_latency,
                    lock_wait,
                );
            }
        };

        if scope_origin.is_none() && scope.is_some() {
            scope_origin = Some(ScopeOrigin::PreExisting);
        }

        let outcome = match &cached {
            Some((prior, v)) if prior == &scope => v.clone(),
            Some(_) => {
                let _ = history.write_event(transition(
                    input,
                    "scope-changed-during-wait".into(),
                    dev_bypass,
                ));
                let v = classifier.classify(input, scope.as_ref(), standing);
                let cloned = v.clone();
                cached = Some((scope.clone(), v));
                cloned
            }
            None => {
                let v = classifier.classify(input, scope.as_ref(), standing);
                let cloned = v.clone();
                cached = Some((scope.clone(), v));
                cloned
            }
        };

        if initial_verdict.is_none() {
            initial_verdict = Some(outcome.verdict.clone());
        }
        let verdict_now = outcome.verdict.clone();
        last_outcome = Some(outcome);

        if matches!(verdict_now, Verdict::Green) {
            return ProcessActionResult {
                scope,
                scope_origin,
                llm_outcome: last_outcome.as_ref().map(RecordedClassifierOutcome::from),
                prompt_shown,
                prompt_latency,
                lock_wait,
                initial_verdict,
                verdict: Verdict::Green,
                override_action: None,
                error_chain: None,
            };
        }
        if matches!(input.agent, Agent::Subagent { .. }) {
            return ProcessActionResult {
                scope,
                scope_origin,
                llm_outcome: last_outcome.as_ref().map(RecordedClassifierOutcome::from),
                prompt_shown,
                prompt_latency,
                lock_wait,
                initial_verdict,
                verdict: verdict_now,
                override_action: None,
                error_chain: None,
            };
        }

        if lock_guard.is_none() {
            let start = Instant::now();
            lock_guard = Some(match prompting.lock.acquire(&key) {
                Ok(g) => g,
                Err(e) => {
                    let chain = join_source_chain(&e);
                    let _ = history.write_event(transition(
                        input,
                        format!("prompt-lock-error: {chain}"),
                        dev_bypass,
                    ));
                    let mut out = ProcessActionResult::red(
                        RedKind::ScopeSetupUnavailable,
                        format!("prompt lock unavailable: {e}"),
                    );
                    out.error_chain = Some(chain);
                    out.initial_verdict = initial_verdict;
                    out.llm_outcome = last_outcome.as_ref().map(RecordedClassifierOutcome::from);
                    out.prompt_shown = prompt_shown;
                    out.prompt_latency = prompt_latency;
                    out.scope = scope;
                    out.scope_origin = scope_origin;
                    return out;
                }
            });
            lock_wait = Some(start.elapsed());
            let _ =
                history.write_event(transition(input, "prompt-lock-acquired".into(), dev_bypass));
            continue;
        }

        let step = if let Verdict::Red {
            kind: RedKind::NoScopeRoot,
            ..
        } = &verdict_now
        {
            prompt_shown = true;
            handle_first_time_setup(
                input,
                store,
                history,
                prompting,
                dev_bypass,
                &mut prompt_latency,
            )
        } else {
            prompt_shown = true;
            handle_multi_button(
                input,
                &verdict_now,
                scope.as_ref(),
                &key,
                store,
                history,
                prompting,
                dev_bypass,
                &mut prompt_latency,
            )
        };

        match step {
            Step::Done {
                verdict,
                override_action,
            } => {
                return ProcessActionResult {
                    scope,
                    scope_origin,
                    llm_outcome: last_outcome.as_ref().map(RecordedClassifierOutcome::from),
                    prompt_shown,
                    prompt_latency,
                    lock_wait,
                    initial_verdict,
                    verdict,
                    override_action,
                    error_chain: None,
                };
            }
            Step::Mutated { new_scope_origin } => {
                if let Some(o) = new_scope_origin {
                    scope_origin = Some(o);
                }
                cached = None;
                continue;
            }
            Step::Retry => continue,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn store_error_outcome<H: ClassificationHistoryWriter>(
    input: &ClassificationInput,
    history: &H,
    error: &(dyn std::error::Error + 'static),
    label: &str,
    dev_bypass: bool,
    scope: Option<Scope>,
    scope_origin: Option<ScopeOrigin>,
    initial_verdict: Option<Verdict>,
    llm_outcome: Option<RecordedClassifierOutcome>,
    prompt_shown: bool,
    prompt_latency: Option<Duration>,
    lock_wait: Option<Duration>,
) -> ProcessActionResult {
    let chain = join_source_chain(error);
    let _ = history.write_event(transition(
        input,
        format!("store-error: {label}: {chain}"),
        dev_bypass,
    ));
    ProcessActionResult {
        scope,
        scope_origin,
        llm_outcome,
        prompt_shown,
        prompt_latency,
        lock_wait,
        initial_verdict,
        verdict: Verdict::Red {
            kind: RedKind::StoreUnavailable,
            reason: format!("agent store unavailable: {error}"),
        },
        override_action: None,
        error_chain: Some(chain),
    }
}

fn transition(
    input: &ClassificationInput,
    message: String,
    dev_bypass: bool,
) -> ClassificationEvent {
    ClassificationEvent {
        ts: time::OffsetDateTime::now_utc(),
        session_id: Some(input.agent.id().clone()),
        correlation_id: Some(input.correlation_id.clone()),
        kind: EventKind::StateTransition,
        message,
        details: BTreeMap::new(),
        dev_bypass,
    }
}

fn join_source_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut chain = err.to_string();
    let mut current = err.source();
    while let Some(c) = current {
        chain.push_str(": ");
        chain.push_str(&c.to_string());
        current = c.source();
    }
    chain
}

#[allow(clippy::too_many_arguments)]
fn handle_first_time_setup<S, H, P, K>(
    input: &ClassificationInput,
    store: &S,
    history: &H,
    prompting: &PromptingDeps<'_, P, K>,
    dev_bypass: bool,
    prompt_latency: &mut Option<Duration>,
) -> Step
where
    S: AgentStore,
    H: ClassificationHistoryWriter,
    P: Prompter,
    K: PromptLock,
{
    let _ = history.write_event(transition(
        input,
        "prompt-update-scope: first-time".into(),
        dev_bypass,
    ));
    let start = Instant::now();
    let result = prompting.prompter.update_scope(None, &input.working_dir);
    *prompt_latency = Some(start.elapsed());
    match result {
        Ok(new_scope) => persist_scope(
            input,
            new_scope,
            store,
            history,
            dev_bypass,
            ScopeOrigin::Fresh,
        ),
        Err(PromptError::Cancelled) => {
            let _ = history.write_event(transition(
                input,
                "user-cancelled-scope-setup".into(),
                dev_bypass,
            ));
            Step::Done {
                verdict: Verdict::Red {
                    kind: RedKind::ScopeSetupCancelled,
                    reason: "scope setup cancelled; nothing was set".into(),
                },
                override_action: None,
            }
        }
        Err(e @ PromptError::Unavailable(_)) => {
            let chain = join_source_chain(&e);
            let _ = history.write_event(transition(
                input,
                format!("prompt-error: first-time: {chain}"),
                dev_bypass,
            ));
            Step::Done {
                verdict: Verdict::Red {
                    kind: RedKind::ScopeSetupUnavailable,
                    reason: format!("scope prompt unavailable: {e}"),
                },
                override_action: None,
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_multi_button<S, H, P, K>(
    input: &ClassificationInput,
    verdict: &Verdict,
    scope: Option<&Scope>,
    key: &AgentId,
    store: &S,
    history: &H,
    prompting: &PromptingDeps<'_, P, K>,
    dev_bypass: bool,
    prompt_latency: &mut Option<Duration>,
) -> Step
where
    S: AgentStore,
    H: ClassificationHistoryWriter,
    P: Prompter,
    K: PromptLock,
{
    let Verdict::Red { kind, reason } = verdict else {
        return Step::Done {
            verdict: verdict.clone(),
            override_action: None,
        };
    };
    let Some(scope) = scope else {
        return Step::Done {
            verdict: Verdict::Red {
                kind: RedKind::StoreUnavailable,
                reason: "scope unexpectedly missing during owner loop".into(),
            },
            override_action: None,
        };
    };

    let _ = history.write_event(transition(
        input,
        format!("prompt-choose: kind={kind:?}"),
        dev_bypass,
    ));
    let start = Instant::now();
    let choice = prompting
        .prompter
        .choose(scope, &input.action, reason, &input.working_dir);
    *prompt_latency = Some(start.elapsed());
    let choice = match choice {
        Ok(c) => c,
        Err(PromptError::Cancelled) => {
            let _ = history.write_event(transition(input, "user-deny".into(), dev_bypass));
            return Step::Done {
                verdict: verdict.clone(),
                override_action: Some(OverrideAction::UserDeny),
            };
        }
        Err(e @ PromptError::Unavailable(_)) => {
            let chain = join_source_chain(&e);
            let _ = history.write_event(transition(
                input,
                format!("prompt-error: multi-button: {chain}"),
                dev_bypass,
            ));
            return Step::Done {
                verdict: verdict.clone(),
                override_action: None,
            };
        }
    };

    match choice {
        VerdictChoice::Deny => {
            let _ = history.write_event(transition(input, "user-deny".into(), dev_bypass));
            Step::Done {
                verdict: verdict.clone(),
                override_action: Some(OverrideAction::UserDeny),
            }
        }
        VerdictChoice::Allow => {
            let _ = history.write_event(transition(input, "user-allow".into(), dev_bypass));
            Step::Done {
                verdict: Verdict::Green,
                override_action: Some(OverrideAction::UserAllow),
            }
        }
        VerdictChoice::UpdateScope => handle_update_scope(
            input,
            scope,
            store,
            history,
            prompting,
            dev_bypass,
            prompt_latency,
        ),
        VerdictChoice::DisableSession => handle_disable_session(
            input,
            key,
            store,
            history,
            prompting,
            dev_bypass,
            prompt_latency,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_update_scope<S, H, P, K>(
    input: &ClassificationInput,
    current: &Scope,
    store: &S,
    history: &H,
    prompting: &PromptingDeps<'_, P, K>,
    dev_bypass: bool,
    prompt_latency: &mut Option<Duration>,
) -> Step
where
    S: AgentStore,
    H: ClassificationHistoryWriter,
    P: Prompter,
    K: PromptLock,
{
    let _ = history.write_event(transition(
        input,
        "prompt-update-scope: edit".into(),
        dev_bypass,
    ));
    let start = Instant::now();
    let result = prompting
        .prompter
        .update_scope(Some(current), &input.working_dir);
    *prompt_latency = Some(start.elapsed());
    match result {
        Ok(new_scope) => persist_scope(
            input,
            new_scope,
            store,
            history,
            dev_bypass,
            ScopeOrigin::Edited,
        ),
        Err(PromptError::Cancelled) => {
            let _ =
                history.write_event(transition(input, "user-cancelled-edit".into(), dev_bypass));
            Step::Retry
        }
        Err(e @ PromptError::Unavailable(_)) => {
            let chain = join_source_chain(&e);
            let _ = history.write_event(transition(
                input,
                format!("prompt-error: edit: {chain}"),
                dev_bypass,
            ));
            Step::Retry
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_disable_session<S, H, P, K>(
    input: &ClassificationInput,
    key: &AgentId,
    store: &S,
    history: &H,
    prompting: &PromptingDeps<'_, P, K>,
    dev_bypass: bool,
    prompt_latency: &mut Option<Duration>,
) -> Step
where
    S: AgentStore,
    H: ClassificationHistoryWriter,
    P: Prompter,
    K: PromptLock,
{
    let _ = history.write_event(transition(
        input,
        "prompt-confirm-disable".into(),
        dev_bypass,
    ));
    let start = Instant::now();
    let result = prompting.prompter.confirm_disable();
    *prompt_latency = Some(start.elapsed());
    match result {
        Ok(true) => {
            let _ = history.write_event(transition(
                input,
                "user-confirmed-disable".into(),
                dev_bypass,
            ));
            if let Err(e) = store.disable(key) {
                let chain = join_source_chain(&e);
                let _ = history.write_event(transition(
                    input,
                    format!("store-error: disable: {chain}"),
                    dev_bypass,
                ));
                return Step::Done {
                    verdict: Verdict::Red {
                        kind: RedKind::StoreUnavailable,
                        reason: format!("agent store unavailable: {e}"),
                    },
                    override_action: None,
                };
            }
            Step::Done {
                verdict: Verdict::Green,
                override_action: Some(OverrideAction::DisableSession),
            }
        }
        Ok(false) => {
            let _ = history.write_event(transition(
                input,
                "user-cancelled-disable".into(),
                dev_bypass,
            ));
            Step::Retry
        }
        Err(e) => {
            let chain = join_source_chain(&e);
            let _ = history.write_event(transition(
                input,
                format!("prompt-error: disable: {chain}"),
                dev_bypass,
            ));
            Step::Retry
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn persist_scope<S, H>(
    input: &ClassificationInput,
    scope: Scope,
    store: &S,
    history: &H,
    dev_bypass: bool,
    origin: ScopeOrigin,
) -> Step
where
    S: AgentStore,
    H: ClassificationHistoryWriter,
{
    let _ = history.write_event(transition(input, "user-typed-scope".into(), dev_bypass));
    if let Err(e) = store.assign_scope(input.agent.id(), scope) {
        let chain = join_source_chain(&e);
        let _ = history.write_event(transition(
            input,
            format!("store-error: assign_scope: {chain}"),
            dev_bypass,
        ));
        return Step::Done {
            verdict: Verdict::Red {
                kind: RedKind::StoreUnavailable,
                reason: format!("agent store unavailable: {e}"),
            },
            override_action: None,
        };
    }
    Step::Mutated {
        new_scope_origin: Some(origin),
    }
}
