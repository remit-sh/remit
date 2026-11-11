use std::collections::BTreeMap;
use std::fmt::Display;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

use anthropic_classifier::AnthropicClassifier;
use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, AgentType, CallId, ClassificationEvent,
    ClassificationHistoryWriter, ClassificationInput, ClassificationRecord, Classifier,
    CorrelationId, Enforced, EventKind, PromptingDeps, RedKind, Verdict, process_action,
    resolve_enforcement,
};
use file_classification_history::FileClassificationHistoryWriter;
use file_scope_store::FileAgentStore;
use file_standing_guidance::FileStandingGuidanceReader;
use prompt_lock::FsPromptLock;
use swift_prompter::SwiftPrompter;

use crate::bundled_scope::{claim_and_assign, discard_pending, stage_pending};
use crate::user_prompt::handle_user_prompt_submit;

mod bundled_scope;
mod user_prompt;

#[cfg(test)]
mod tests;

/// Scope-bearing parent tools → the field in their `tool_input` that carries
/// the bundled scope for the child they spawn or re-wake. Hook lifts this on
/// the parent's `PreToolUse` and stages it under the parent's id; the child's
/// start hook claims it. The text is distilled by the LLM before staging
/// (TN-2026-017). `Agent` covers fresh in-process spawns and team
/// peer-process spawns; `SendMessage` covers re-wakes of an existing subagent,
/// where each round arrives as a new `SubagentStart` with a fresh `agent_id`.
/// Vocab: [Bundled scope](../../../CONTEXT.md#agents).
const BUNDLED_SCOPE_FIELDS: &[(&str, &str)] = &[("Agent", "prompt"), ("SendMessage", "content")];

/// JSON keys dropped from `tool_input` before classification. Agent-controlled
/// narration that biases the classifier without grounding behaviour. Walked
/// recursively, matched case-insensitively. Tool-agnostic by design — a tool
/// using one of these names for behavioural input pays the false-Red cost in
/// exchange for keeping the rule generic.
const NARRATION_FIELDS: &[&str] = &[
    "description",
    "reason",
    "note",
    "comment",
    "intent",
    "purpose",
    "rationale",
    "justification",
    "summary",
];

#[derive(serde::Deserialize)]
struct PreToolUsePayload {
    session_id: String,
    /// Set only for in-process subagents — collapsed into the [`Agent`] identity.
    agent_id: Option<String>,
    /// Runtime-supplied role label. Present for in-process and peer-process
    /// subagents; absent for root sessions. Validated downstream into
    /// [`AgentType`] for **Agent template** lookup (TN-2026-009).
    agent_type: Option<String>,

    tool_use_id: Option<String>,
    tool_name: String,
    tool_input: serde_json::Value,
    cwd: PathBuf,
}

#[derive(serde::Deserialize)]
struct SubagentStartPayload {
    session_id: String,
    agent_id: String,
}

#[derive(serde::Deserialize)]
struct SessionStartPayload {
    session_id: String,
}

fn main() {
    let hook_started = Instant::now();
    let correlation_id = CorrelationId::new();

    let mut buf = String::new();
    fail_closed("read stdin", std::io::stdin().read_to_string(&mut buf));
    let raw_payload: serde_json::Value =
        fail_closed("parse hook payload", serde_json::from_str(&buf));

    let event = raw_payload
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Dispatched before any `fail_closed` setup: this event yields no verdict
    // and exit 2 would block the user's prompt. Failure leaves the agent
    // unscoped, which the first tool call turns into the dialog or Red.
    if event == "UserPromptSubmit" {
        if let Err(e) = handle_user_prompt_submit(raw_payload, &correlation_id) {
            eprintln!("remit: UserPromptSubmit: {e}; no scope assigned");
        }
        return;
    }

    let cfg = fail_closed("config", remit_config::load());
    let agents_dir = fail_closed("agents dir", remit_config::agents_dir());

    let log_path = fail_closed("log path", remit_config::log_path());
    let logger = FileClassificationHistoryWriter::new(log_path);

    warn_on_history_err(logger.write_event(lifecycle_event(
        format!("hook-event: {event}"),
        BTreeMap::new(),
        Some(&correlation_id),
        cfg.dev_bypass,
    )));

    let agent_store = FileAgentStore::new(agents_dir.clone());
    let prompt_lock = FsPromptLock::new(agents_dir);
    let prompter = SwiftPrompter::new();

    match event.as_str() {
        "PreToolUse" => {
            let payload: PreToolUsePayload =
                fail_closed("parse PreToolUse", serde_json::from_value(raw_payload));
            let llm = build_llm_classifier(&cfg);
            handle_pre_tool_use(
                payload,
                &cfg,
                &agent_store,
                &prompt_lock,
                &prompter,
                &logger,
                llm,
                correlation_id.clone(),
                hook_started,
            );
        }
        "SubagentStart" => {
            if let Ok(payload) = serde_json::from_value::<SubagentStartPayload>(raw_payload) {
                let parent = fail_closed(
                    "SubagentStart parent id",
                    AgentId::try_new(payload.session_id),
                );
                let child =
                    fail_closed("SubagentStart agent id", AgentId::try_new(payload.agent_id));
                claim_and_assign(&agent_store, &parent, &child, "SubagentStart", &logger);
            }
        }
        "SessionStart" => {
            if let Ok(payload) = serde_json::from_value::<SessionStartPayload>(raw_payload) {
                let child = fail_closed(
                    "SessionStart session id",
                    AgentId::try_new(payload.session_id),
                );
                if let Some(parent) = parent_from_env(&child) {
                    claim_and_assign(&agent_store, &parent, &child, "SessionStart", &logger);
                }
                // No env var → root session. Nothing to claim; root prompts via PreToolUse.
            }
        }
        _ => {} // Hook is registered for events we don't act on; ignore.
    }
}

#[allow(clippy::too_many_arguments)] // glue function in the composition root
fn handle_pre_tool_use(
    payload: PreToolUsePayload,
    cfg: &remit_config::RemitConfig,
    agent_store: &FileAgentStore,
    prompt_lock: &FsPromptLock,
    prompter: &SwiftPrompter,
    logger: &FileClassificationHistoryWriter,
    llm: AnthropicClassifier,
    correlation_id: CorrelationId,
    hook_started: Instant,
) {
    let payload_session = fail_closed(
        "PreToolUse session id",
        AgentId::try_new(payload.session_id),
    );

    let bundled_text = bundled_text_from(&payload.tool_name, &payload.tool_input);
    let bundled_scope_present = bundled_text.is_some_and(|text| !text.trim().is_empty());
    if let Some(text) = bundled_text {
        stage_pending(&payload_session, text, &llm, agent_store, logger);
    }
    let spawn_parent = bundled_text.is_some().then(|| payload_session.clone());

    let args_size_pre = u32::try_from(payload.tool_input.to_string().len()).unwrap_or(u32::MAX);
    let mut tool_input = payload.tool_input;
    let narration_fields_stripped = strip_narration(&mut tool_input);
    let stripped_serialised = tool_input.to_string();
    let args_size_post = u32::try_from(stripped_serialised.len()).unwrap_or(u32::MAX);

    let agent = agent_from(
        payload_session,
        payload
            .agent_id
            .map(|agent_id| fail_closed("PreToolUse agent id", AgentId::try_new(agent_id))),
        parent_from_env,
    );

    let cwd = payload.cwd.clone();
    let tool_name_kind = ActionKind::new(payload.tool_name.clone());
    let call_id = payload.tool_use_id.clone().map(CallId::new);
    let classifier_input = ClassificationInput {
        agent: agent.clone(),
        action: Action {
            call_id: call_id.clone(),
            kind: tool_name_kind.clone(),
            args: ActionArgs::new(stripped_serialised.clone()),
        },
        working_dir: cwd.clone(),
        correlation_id: correlation_id.clone(),
    };

    let agent_type = parse_agent_type(payload.agent_type.as_deref());
    if payload.agent_type.is_some() && agent_type.is_none() {
        let mut details = BTreeMap::new();
        // Cap attacker-controlled `raw` value at 1 KiB before it goes into the
        // audit row. NDJSON line atomicity is preserved by serde escaping;
        // this guards against an attacker shipping multi-MB agent_type strings
        // that bloat the log.
        let raw = truncate_for_details(payload.agent_type.as_deref().unwrap_or(""));
        details.insert("raw".into(), raw);
        warn_on_history_err(logger.write_event(lifecycle_event(
            "invalid-agent-type".into(),
            details,
            Some(&correlation_id),
            cfg.dev_bypass,
        )));
    }

    let guidance_reader =
        FileStandingGuidanceReader::new(cwd.clone()).with_agent_type(agent_type.clone());
    let classifier = Classifier::new(llm);
    let prompting = PromptingDeps {
        prompter,
        lock: prompt_lock,
    };
    let outcome = process_action(
        &classifier_input,
        agent_store,
        &classifier,
        logger,
        &prompting,
        &guidance_reader,
        cfg.dev_bypass,
    );

    let (enforced, dev_override) = resolve_enforcement(&outcome.verdict, cfg.dev_bypass);
    let override_action = dev_override.or(outcome.override_action);

    let record = ClassificationRecord {
        ts: time::OffsetDateTime::now_utc(),
        session_id: classifier_input.agent.id().clone(),
        parent_id: classifier_input.agent.parent_id().cloned(),
        agent_type,
        working_dir: cwd,
        call_id,
        correlation_id: correlation_id.clone(),
        pid: std::process::id(),
        hook_duration: hook_started.elapsed(),
        tool_name: tool_name_kind,
        // Always captured during pre-MVP for forensic depth. Revisit before
        // public shipping (CLAUDE.md security policy: operational data may be
        // persisted locally; the 0600 audit log boundary holds the line).
        tool_input: Some(stripped_serialised),
        bundled_scope_present,
        narration_fields_stripped,
        args_size_pre,
        args_size_post,
        scope: outcome.scope,
        scope_origin: outcome.scope_origin,
        llm_outcome: outcome.llm_outcome,
        prompt_shown: outcome.prompt_shown,
        prompt_latency: outcome.prompt_latency,
        lock_wait: outcome.lock_wait,
        initial_verdict: outcome.initial_verdict,
        verdict: outcome.verdict.clone(),
        enforced,
        override_action,
        error_chain: outcome.error_chain.as_deref().map(truncate_error_chain),
        dev_bypass: cfg.dev_bypass,
        details: BTreeMap::new(),
    };
    warn_on_history_err(logger.write(record));

    // A denied spawn never starts its child; drop what was staged for it so
    // a later child of this parent cannot claim it.
    if let (Enforced::Deny, Some(parent)) = (enforced, &spawn_parent) {
        discard_pending(parent, agent_store, logger);
    }

    match (enforced, &outcome.verdict) {
        (Enforced::Permit, Verdict::Green) => {}
        (Enforced::Permit, Verdict::Red { reason, .. }) => {
            let line = format_red_for_agent(reason, agent_store, classifier_input.agent.id());
            eprintln!("[dev-bypass] would have denied: {line}");
        }
        (Enforced::Deny, Verdict::Red { reason, kind }) => {
            let line = format_red_for_agent(reason, agent_store, classifier_input.agent.id());
            let line = match dev_bypass_hint(kind) {
                Some(hint) => format!("{line}\n\n{hint}"),
                None => line,
            };
            eprintln!("{line}");
            std::process::exit(2);
        }
        (Enforced::Deny, Verdict::Green) => {
            // resolve_enforcement never returns this; defence-in-depth.
            std::process::exit(2);
        }
    }
}

fn lifecycle_event(
    message: String,
    details: BTreeMap<String, String>,
    correlation_id: Option<&CorrelationId>,
    dev_bypass: bool,
) -> ClassificationEvent {
    ClassificationEvent {
        ts: time::OffsetDateTime::now_utc(),
        session_id: None,
        correlation_id: correlation_id.cloned(),
        kind: EventKind::Lifecycle,
        message,
        details,
        dev_bypass,
    }
}

/// Per-write history-error policy at the composition root. Matches ADR-0002
/// best-effort logging: never block the verdict on a write failure, but
/// surface failures to stderr so an operator knows the audit trail is
/// degraded. SQLite (#38) will tighten this to fail-closed by trapping the
/// `Err` here.
fn warn_on_history_err(result: Result<(), domain::HistoryError>) {
    if let Err(e) = result {
        eprintln!("remit: audit write failed: {e}");
    }
}

/// Cap attacker-controlled string values before they land in an audit row's
/// `details` map. 1 KiB is generous for diagnostic context but bounds the
/// damage a hostile payload can do to log size.
const DETAILS_VALUE_MAX_BYTES: usize = 1024;
const ERROR_CHAIN_MAX_BYTES: usize = 2048;

fn truncate_for_details(s: &str) -> String {
    truncate_at_char_boundary(s, DETAILS_VALUE_MAX_BYTES).to_string()
}

fn truncate_error_chain(s: &str) -> String {
    truncate_at_char_boundary(s, ERROR_CHAIN_MAX_BYTES).to_string()
}

fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// User-facing recovery hint for Red kinds where the user can unstick the
/// system by flipping a config flag. Only attached for failure-shaped Reds
/// (prompter unavailable); never for genuine "out of scope" Reds, which would
/// confusingly suggest bypassing a correct denial.
fn dev_bypass_hint(kind: &RedKind) -> Option<&'static str> {
    match kind {
        RedKind::ScopeSetupUnavailable => {
            Some("To bypass while debugging, set `dev_bypass = true` in ~/.remit/config.toml.")
        }
        RedKind::NoScopeRoot
        | RedKind::ScopeSetupCancelled
        | RedKind::SubagentMissingScope
        | RedKind::StoreUnavailable
        | RedKind::OutOfScope
        | RedKind::ClassifierUnavailable
        | RedKind::StandingGuidanceUnavailable => None,
    }
}

/// Append the agent's current scope to the LLM's reason so the agent (which has
/// no read access to its own scope row) gets a reminder of what it's bound to.
/// Scope read failure is fail-soft on this path — the Red has already been
/// decided; we still emit the reason. The LLM is instructed not to restate
/// scope in `CLASSIFIER_GUIDANCE`, so without this line the agent would lose
/// the only feedback channel for scope content.
fn format_red_for_agent<S: domain::AgentStore>(
    reason: &str,
    agent_store: &S,
    agent_id: &AgentId,
) -> String {
    match agent_store.scope_for(agent_id) {
        Ok(Some(scope)) => format!("{reason}\n\nYour scope: {}", scope.as_ref()),
        Ok(None) | Err(_) => reason.to_string(),
    }
}

/// Parse the raw `agent_type` from the hook payload into a validated
/// [`AgentType`]. `None` for missing or invalid input; the caller logs
/// invalid-but-present cases. Validation rejects path-traversal and unusual
/// charset by construction, so unknown shapes silently fall back to "no
/// template loaded" rather than failing the action.
fn parse_agent_type(raw: Option<&str>) -> Option<AgentType> {
    raw.and_then(|s| AgentType::try_new(s.to_string()).ok())
}

/// Recover the parent's [`AgentId`] from the `CLAUDE_SESSION_ID` env var. Used for
/// peer-process subagents (team sessions) which don't receive parent_id in payload.
fn parent_from_env(self_id: &AgentId) -> Option<AgentId> {
    parent_from_env_value(std::env::var("CLAUDE_SESSION_ID").ok(), self_id)
}

fn parent_from_env_value(env: Option<String>, self_id: &AgentId) -> Option<AgentId> {
    env.and_then(|s| AgentId::try_new(s).ok())
        .filter(|p| p != self_id)
}

/// Recursively drop [`NARRATION_FIELDS`] from a JSON value in place. Object
/// keys are matched case-insensitively. Descends into nested objects and into
/// objects inside arrays. Non-object, non-array roots are unchanged. Returns
/// the number of fields stripped (audit data).
fn strip_narration(value: &mut serde_json::Value) -> u32 {
    let mut count = 0u32;
    strip_narration_inner(value, &mut count);
    count
}

fn strip_narration_inner(value: &mut serde_json::Value, count: &mut u32) {
    match value {
        serde_json::Value::Object(map) => {
            map.retain(|k, v| {
                if NARRATION_FIELDS.iter().any(|f| f.eq_ignore_ascii_case(k)) {
                    *count = count.saturating_add(1);
                    return false;
                }
                strip_narration_inner(v, count);
                true
            });
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_narration_inner(v, count);
            }
        }
        _ => {}
    }
}

/// Lift the bundled-scope text out of a spawn-shaped tool call. `None` for
/// non-spawn tools. A spawn-shaped tool always yields text: a missing or
/// non-string field is the empty string, so staging still runs and clears
/// the parent's pending slot, leaving the child unscoped (Red).
fn bundled_text_from<'a>(tool_name: &str, tool_input: &'a serde_json::Value) -> Option<&'a str> {
    let field = BUNDLED_SCOPE_FIELDS
        .iter()
        .find_map(|(t, f)| (*t == tool_name).then_some(*f))?;
    Some(tool_input.get(field).and_then(|v| v.as_str()).unwrap_or(""))
}

/// Identity collapse: translate Claude Code ids into the domain [`Agent`].
/// A payload `agent_id` is an in-process subagent of the session; a session
/// with an env-supplied parent is a peer-process subagent; otherwise root.
fn agent_from(
    session: AgentId,
    agent_id: Option<AgentId>,
    parent_of: impl FnOnce(&AgentId) -> Option<AgentId>,
) -> Agent {
    match agent_id {
        Some(agent_id) => Agent::subagent(agent_id, session),
        None => match parent_of(&session) {
            Some(parent) => Agent::subagent(session, parent),
            None => Agent::root(session),
        },
    }
}

/// Unwrap or exit 2 with `remit: <ctx>: <err>; deny`. The fail-closed primitive
/// for the boundary: any parse / id / config failure denies the action.
fn fail_closed<T, E: Display>(ctx: &str, result: Result<T, E>) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("remit: {ctx}: {e}; deny");
        std::process::exit(2);
    })
}

/// Dispatch on the configured classifier adapter and build it. Adapter-specific
/// resolution (defaults, env-var fallbacks) lives inside each adapter.
/// Missing config that prevents construction denies the action — we never
/// classify without a working classifier.
fn build_llm_classifier(cfg: &remit_config::RemitConfig) -> AnthropicClassifier {
    fail_closed("classifier", try_build_llm_classifier(cfg))
}

/// The single adapter-selection site. Callers decide what a build failure
/// means: deny for `PreToolUse`, "no scope assigned" for `UserPromptSubmit`.
fn try_build_llm_classifier(
    cfg: &remit_config::RemitConfig,
) -> Result<AnthropicClassifier, anthropic_classifier::ConfigError> {
    match cfg.classifier.adapter {
        remit_config::ClassifierAdapter::Anthropic => AnthropicClassifier::from_config(
            cfg.classifier.anthropic.api_key.clone(),
            cfg.classifier.anthropic.model.clone(),
        ),
    }
}
