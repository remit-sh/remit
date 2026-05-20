//! Orchestration: turn a [`ClassificationInput`] into a [`Verdict`] by wiring
//! the domain's ports together. Composition roots call [`process_action`].

use crate::{
    Agent, AgentId, AgentStore, ClassificationHistoryWriter, ClassificationInput, Classifier,
    LlmClassifier, PromptError, PromptLock, Prompter, PromptingDeps, RedKind, Scope,
    ScopeStoreError, StandingGuidance, StandingGuidanceReader, Verdict, VerdictChoice,
};

pub fn resolve_scope<S: AgentStore>(
    store: &S,
    agent: &Agent,
) -> Result<Option<Scope>, ScopeStoreError> {
    store.scope_for(&agent.id)
}

/// Subagents inherit the root's id by walking one level up;
/// [`Agent::subagent`] already collapses self-referential parents to root.
fn root_key(agent: &Agent) -> &AgentId {
    agent.parent_id.as_ref().unwrap_or(&agent.id)
}

/// Owns the audit trail: exactly one `record_incoming` + one `record_outcome`,
/// `record_error` on any port failure, and `record_event` for in-flight transitions
/// inside the prompt flow.
#[allow(clippy::too_many_arguments)] // composition glue: each port is a distinct collaborator
pub fn process_action<S, L, C, P, K, G>(
    input: &ClassificationInput,
    store: &S,
    classifier: &Classifier<C>,
    logger: &L,
    prompting: &PromptingDeps<'_, P, K>,
    guidance_reader: &G,
) -> Verdict
where
    S: AgentStore,
    L: ClassificationHistoryWriter,
    C: LlmClassifier,
    P: Prompter,
    K: PromptLock,
    G: StandingGuidanceReader,
{
    logger.record_incoming(input);
    let verdict = match guidance_reader.read() {
        Ok(guidance) => process_inner(input, store, classifier, logger, prompting, &guidance),
        Err(e) => {
            logger.record_error(input, &e);
            Verdict::Red {
                kind: RedKind::StandingGuidanceUnavailable,
                reason: format!("standing guidance unavailable: {e}"),
            }
        }
    };
    logger.record_outcome(input, &verdict);
    verdict
}

/// `Mutated` invalidates the classify cache (we wrote new state); `Retry`
/// re-loops with cache intact (cheap re-prompt after a cancel).
enum Step {
    Done(Verdict),
    Mutated,
    Retry,
}

#[allow(clippy::too_many_arguments)] // each port is a distinct collaborator
fn process_inner<S, L, C, P, K>(
    input: &ClassificationInput,
    store: &S,
    classifier: &Classifier<C>,
    logger: &L,
    prompting: &PromptingDeps<'_, P, K>,
    standing: &StandingGuidance,
) -> Verdict
where
    S: AgentStore,
    L: ClassificationHistoryWriter,
    C: LlmClassifier,
    P: Prompter,
    K: PromptLock,
{
    let key = root_key(&input.agent).clone();
    let mut lock_guard: Option<K::Guard> = None;
    let mut cached: Option<(Option<Scope>, Verdict)> = None;

    loop {
        match store.is_disabled(&key) {
            Ok(true) => {
                let event = if lock_guard.is_some() {
                    "disabled-during-wait"
                } else {
                    "session-disabled"
                };
                logger.record_event(input, event);
                return Verdict::Green;
            }
            Ok(false) => {}
            Err(e) => {
                logger.record_error(input, &e);
                return Verdict::Red {
                    kind: RedKind::StoreUnavailable,
                    reason: format!("agent store unavailable: {e}"),
                };
            }
        }

        let scope = match resolve_scope(store, &input.agent) {
            Ok(s) => s,
            Err(e) => {
                logger.record_error(input, &e);
                return Verdict::Red {
                    kind: RedKind::StoreUnavailable,
                    reason: format!("agent store unavailable: {e}"),
                };
            }
        };

        // Cache miss with a prior entry = scope changed during the lock wait,
        // log it; without = first iteration or we just invalidated.
        let verdict = match &cached {
            Some((prior, v)) if prior == &scope => v.clone(),
            Some(_) => {
                logger.record_event(input, "scope-changed-during-wait");
                let v = classifier.classify(input, scope.as_ref(), standing);
                cached = Some((scope.clone(), v.clone()));
                v
            }
            None => {
                let v = classifier.classify(input, scope.as_ref(), standing);
                cached = Some((scope.clone(), v.clone()));
                v
            }
        };

        if matches!(verdict, Verdict::Green) {
            return Verdict::Green;
        }
        if input.agent.has_parent() {
            return verdict;
        }

        if lock_guard.is_none() {
            lock_guard = Some(match prompting.lock.acquire(&key) {
                Ok(g) => g,
                Err(e) => {
                    logger.record_error(input, &e);
                    return Verdict::Red {
                        kind: RedKind::ScopeSetupUnavailable,
                        reason: format!("prompt lock unavailable: {e}"),
                    };
                }
            });
            logger.record_event(input, "prompt-lock-acquired");
            continue;
        }

        let step = if let Verdict::Red {
            kind: RedKind::NoScopeRoot,
            ..
        } = &verdict
        {
            handle_first_time_setup(input, store, logger, prompting)
        } else {
            handle_multi_button(
                input,
                &verdict,
                scope.as_ref(),
                &key,
                store,
                logger,
                prompting,
            )
        };

        match step {
            Step::Done(v) => return v,
            Step::Mutated => {
                cached = None;
                continue;
            }
            Step::Retry => continue,
        }
    }
}

fn handle_first_time_setup<S, L, P, K>(
    input: &ClassificationInput,
    store: &S,
    logger: &L,
    prompting: &PromptingDeps<'_, P, K>,
) -> Step
where
    S: AgentStore,
    L: ClassificationHistoryWriter,
    P: Prompter,
    K: PromptLock,
{
    logger.record_event(input, "prompt-update-scope: first-time");
    match prompting.prompter.update_scope(None, &input.working_dir) {
        Ok(new_scope) => persist_scope(input, new_scope, store, logger),
        Err(PromptError::Cancelled) => {
            logger.record_event(input, "user-cancelled-scope-setup");
            Step::Done(Verdict::Red {
                kind: RedKind::ScopeSetupCancelled,
                reason: "scope setup cancelled; nothing was set".into(),
            })
        }
        Err(e @ PromptError::Unavailable(_)) => {
            logger.record_error(input, &e);
            Step::Done(Verdict::Red {
                kind: RedKind::ScopeSetupUnavailable,
                reason: format!("scope prompt unavailable: {e}"),
            })
        }
    }
}

#[allow(clippy::too_many_arguments)] // glue helper in the prompt flow
fn handle_multi_button<S, L, P, K>(
    input: &ClassificationInput,
    verdict: &Verdict,
    scope: Option<&Scope>,
    key: &AgentId,
    store: &S,
    logger: &L,
    prompting: &PromptingDeps<'_, P, K>,
) -> Step
where
    S: AgentStore,
    L: ClassificationHistoryWriter,
    P: Prompter,
    K: PromptLock,
{
    let Verdict::Red { kind, reason } = verdict else {
        return Step::Done(verdict.clone());
    };
    let Some(scope) = scope else {
        // Non-NoScopeRoot Red implies scope was present at classify time;
        // race-y absence here is fail-closed.
        return Step::Done(Verdict::Red {
            kind: RedKind::StoreUnavailable,
            reason: "scope unexpectedly missing during owner loop".into(),
        });
    };

    logger.record_event(input, &format!("prompt-choose: kind={kind:?}"));
    let choice = match prompting
        .prompter
        .choose(scope, &input.action, reason, &input.working_dir)
    {
        Ok(c) => c,
        Err(PromptError::Cancelled) => {
            logger.record_event(input, "user-deny");
            return Step::Done(verdict.clone());
        }
        Err(e @ PromptError::Unavailable(_)) => {
            logger.record_error(input, &e);
            return Step::Done(verdict.clone());
        }
    };

    match choice {
        VerdictChoice::Deny => {
            logger.record_event(input, "user-deny");
            Step::Done(verdict.clone())
        }
        VerdictChoice::Allow => {
            logger.record_event(input, "user-allow");
            Step::Done(Verdict::Green)
        }
        VerdictChoice::UpdateScope => handle_update_scope(input, scope, store, logger, prompting),
        VerdictChoice::DisableSession => {
            handle_disable_session(input, key, store, logger, prompting)
        }
    }
}

fn handle_update_scope<S, L, P, K>(
    input: &ClassificationInput,
    current: &Scope,
    store: &S,
    logger: &L,
    prompting: &PromptingDeps<'_, P, K>,
) -> Step
where
    S: AgentStore,
    L: ClassificationHistoryWriter,
    P: Prompter,
    K: PromptLock,
{
    logger.record_event(input, "prompt-update-scope: edit");
    match prompting
        .prompter
        .update_scope(Some(current), &input.working_dir)
    {
        Ok(new_scope) => persist_scope(input, new_scope, store, logger),
        Err(PromptError::Cancelled) => {
            logger.record_event(input, "user-cancelled-edit");
            Step::Retry
        }
        Err(e @ PromptError::Unavailable(_)) => {
            logger.record_error(input, &e);
            Step::Retry
        }
    }
}

fn handle_disable_session<S, L, P, K>(
    input: &ClassificationInput,
    key: &AgentId,
    store: &S,
    logger: &L,
    prompting: &PromptingDeps<'_, P, K>,
) -> Step
where
    S: AgentStore,
    L: ClassificationHistoryWriter,
    P: Prompter,
    K: PromptLock,
{
    logger.record_event(input, "prompt-confirm-disable");
    match prompting.prompter.confirm_disable() {
        Ok(true) => {
            logger.record_event(input, "user-confirmed-disable");
            if let Err(e) = store.disable(key) {
                logger.record_error(input, &e);
                return Step::Done(Verdict::Red {
                    kind: RedKind::StoreUnavailable,
                    reason: format!("agent store unavailable: {e}"),
                });
            }
            Step::Done(Verdict::Green)
        }
        Ok(false) => {
            logger.record_event(input, "user-cancelled-disable");
            Step::Retry
        }
        Err(e) => {
            logger.record_error(input, &e);
            Step::Retry
        }
    }
}

fn persist_scope<S, L>(input: &ClassificationInput, scope: Scope, store: &S, logger: &L) -> Step
where
    S: AgentStore,
    L: ClassificationHistoryWriter,
{
    logger.record_event(input, "user-typed-scope");
    if let Err(e) = store.assign_scope(&input.agent.id, scope) {
        logger.record_error(input, &e);
        return Step::Done(Verdict::Red {
            kind: RedKind::StoreUnavailable,
            reason: format!("agent store unavailable: {e}"),
        });
    }
    Step::Mutated
}
