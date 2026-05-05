use std::fmt::Display;
use std::io::Read;
use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, CallId, ClassificationInput, Classifier,
    PendingScopeStore, Scope, Verdict, process_action,
};
use file_logger::FileLogger;
use file_scope_store::{FilePendingScopeStore, FileScopeStore};
use osascript_prompter::OsascriptPrompter;

#[cfg(test)]
mod tests;

/// Spawn-shaped tools → the field in their `tool_input` that carries the bundled
/// scope for the child they spawn. Hook lifts this on the parent's `PreToolUse`
/// and stages it under the parent's id; the child's start hook claims it.
/// Vocab: [Bundled scope](../../../../docs/UBIQUITOUS_LANGUAGE.md#agents).
const BUNDLED_SCOPE_FIELDS: &[(&str, &str)] = &[("Agent", "prompt")];

#[derive(serde::Deserialize)]
struct PreToolUsePayload {
    session_id: String,
    /// Set only for in-process subagents — collapsed into the [`Agent`] identity.
    agent_id: Option<String>,

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
    let cfg = fail_closed("config", remit_config::load());
    let scope_dir = fail_closed("scope dir", remit_config::scope_dir());
    let pending_dir = fail_closed("pending scope dir", remit_config::pending_scope_dir());

    let log_path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../remit.log"));
    let logger = FileLogger::new(log_path);

    let mut buf = String::new();
    fail_closed("read stdin", std::io::stdin().read_to_string(&mut buf));
    let raw_payload: serde_json::Value =
        fail_closed("parse hook payload", serde_json::from_str(&buf));

    let event = raw_payload
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    logger.log_line(&format!("hook-event: {event}"));

    let scope_store = FileScopeStore::new(scope_dir);
    let pending_store = FilePendingScopeStore::new(pending_dir);

    match event.as_str() {
        "PreToolUse" => {
            let payload: PreToolUsePayload =
                fail_closed("parse PreToolUse", serde_json::from_value(raw_payload));
            handle_pre_tool_use(payload, &cfg, &scope_store, &pending_store, &logger);
        }
        "SubagentStart" => {
            if let Ok(payload) = serde_json::from_value::<SubagentStartPayload>(raw_payload) {
                let parent = fail_closed(
                    "SubagentStart parent id",
                    AgentId::try_new(payload.session_id),
                );
                let child =
                    fail_closed("SubagentStart agent id", AgentId::try_new(payload.agent_id));
                claim_and_assign(
                    &pending_store,
                    &scope_store,
                    &parent,
                    &child,
                    "SubagentStart",
                    &logger,
                );
            }
        }
        "SessionStart" => {
            if let Ok(payload) = serde_json::from_value::<SessionStartPayload>(raw_payload) {
                let child = fail_closed(
                    "SessionStart session id",
                    AgentId::try_new(payload.session_id),
                );
                if let Some(parent) = parent_from_env(&child) {
                    claim_and_assign(
                        &pending_store,
                        &scope_store,
                        &parent,
                        &child,
                        "SessionStart",
                        &logger,
                    );
                }
                // No env var → root session. Nothing to claim; root prompts via PreToolUse.
            }
        }
        _ => {} // Hook is registered for events we don't act on; ignore.
    }
}

fn handle_pre_tool_use(
    payload: PreToolUsePayload,
    cfg: &remit_config::RemitConfig,
    scope_store: &FileScopeStore,
    pending_store: &FilePendingScopeStore,
    logger: &FileLogger,
) {
    let payload_session = fail_closed(
        "PreToolUse session id",
        AgentId::try_new(payload.session_id),
    );

    // Bundled scope: when a parent calls a spawn-shaped tool, the scope for the
    // upcoming child is sitting right there in the tool args. Stage it so the
    // child's start hook can claim it. The spawn call itself still falls through
    // to classification under the parent's own scope.
    if let Some(text) = bundled_scope_from(&payload.tool_name, &payload.tool_input) {
        stage_pending(&payload_session, text, pending_store, logger);
    }

    // Identity collapse: in-process subagents have their own `agent_id` that
    // uniquely identifies them. We file scopes under it and treat the payload
    // session id as the parent for inheritance walks (and for matching pending rows).
    let agent = match payload.agent_id {
        Some(agent_id) => Agent::subagent(
            fail_closed("PreToolUse agent id", AgentId::try_new(agent_id)),
            payload_session,
        ),
        None => match parent_from_env(&payload_session) {
            Some(parent) => Agent::subagent(payload_session, parent),
            None => Agent::root(payload_session),
        },
    };

    let classifier_input = ClassificationInput {
        agent,
        action: Action {
            call_id: payload.tool_use_id.map(CallId::new),
            kind: ActionKind::new(payload.tool_name),
            args: ActionArgs::new(payload.tool_input.to_string()),
        },
        working_dir: payload.cwd,
    };

    let prompter = OsascriptPrompter::new();
    let classifier = Classifier::new(logger.clone());
    let verdict = process_action(
        &classifier_input,
        scope_store,
        &prompter,
        scope_store,
        &classifier,
    );

    match verdict {
        Verdict::Green => {}
        Verdict::Red { reason, .. } if cfg.dev_bypass => {
            eprintln!("[dev-bypass] would have denied: {reason}");
        }
        Verdict::Red { reason, .. } => {
            eprintln!("{reason}");
            std::process::exit(2);
        }
    }
}

/// Record the bundled scope under the parent's id, to be claimed by the child's
/// start hook. Empty-after-trim text is rejected at the boundary; pending-store
/// failure exits 2 so the parent's spawn doesn't proceed thinking it succeeded.
fn stage_pending(
    parent: &AgentId,
    scope_text: String,
    pending_store: &FilePendingScopeStore,
    logger: &FileLogger,
) {
    let scope = match Scope::try_new(&scope_text) {
        Ok(s) => s,
        Err(_) => {
            // Fail-closed at the boundary: refuse to stage an empty/whitespace scope.
            // The child will hit `SubagentMissingScope` at first tool call, which is
            // strictly safer than binding a meaningless string.
            logger.log_line(&format!(
                "stage-pending: parent={parent} skipped (empty bundled scope)"
            ));
            return;
        }
    };
    match pending_store.set(parent, scope) {
        Ok(()) => logger.log_line(&format!(
            "stage-pending: parent={parent} scope={scope_text:?}"
        )),
        Err(e) => {
            // Fail-closed: deny the spawn so the parent doesn't proceed thinking
            // the scope was set.
            logger.log_line(&format!("stage-pending: parent={parent} write-failed: {e}"));
            std::process::exit(2);
        }
    }
}

/// Claim the parent's pending row and bind the scope to the child's id.
/// Hit → `ScopeWriter::assign` under the child; miss → child will be blocked at
/// first tool call. Exits 2 if `assign` fails after `take` consumed the row.
#[allow(clippy::too_many_arguments)] // glue function in the composition root
fn claim_and_assign(
    pending_store: &FilePendingScopeStore,
    scope_store: &FileScopeStore,
    parent: &AgentId,
    child: &AgentId,
    hook: &str,
    logger: &FileLogger,
) {
    use domain::ScopeWriter;

    let claim = pending_store.take(parent);
    let outcome = match &claim {
        Ok(Some(scope)) => match scope_store.assign(child, scope.clone()) {
            Ok(()) => format!("hit scope={:?}", scope.as_ref()),
            Err(e) => format!("hit-but-assign-failed scope={:?} error={e}", scope.as_ref()),
        },
        Ok(None) => "miss (no pending row; child will be blocked at first tool call)".into(),
        Err(e) => format!("error {e}"),
    };
    logger.log_line(&format!(
        "claim-pending: hook={hook} parent={parent} child={child} {outcome}"
    ));

    // Fail-closed signal: the pending row was consumed but the child's scope row
    // wasn't written. The child will hit `SubagentMissingScope` at first tool
    // call (verdict-correct) but the operator needs to see the underlying cause.
    if let Ok(Some(_)) = claim
        && outcome.starts_with("hit-but-assign-failed")
    {
        eprintln!("remit: {hook} claim_and_assign: assign failed; child will run unscoped");
        std::process::exit(2);
    }
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

/// Extract bundled-scope text for a spawn-shaped tool call. Returns `None` for
/// non-spawn tools, missing/non-string fields, and empty/whitespace text —
/// all of which leave the child unscoped, so the classifier emits Red.
fn bundled_scope_from(tool_name: &str, tool_input: &serde_json::Value) -> Option<String> {
    let field = BUNDLED_SCOPE_FIELDS
        .iter()
        .find_map(|(t, f)| (*t == tool_name).then_some(*f))?;
    tool_input
        .get(field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
}

/// Unwrap or exit 2 with `remit: <ctx>: <err>; deny`. The fail-closed primitive
/// for the boundary: any parse / id / config failure denies the action.
fn fail_closed<T, E: Display>(ctx: &str, result: Result<T, E>) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("remit: {ctx}: {e}; deny");
        std::process::exit(2);
    })
}
