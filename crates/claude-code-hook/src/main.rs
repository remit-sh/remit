use std::fmt::Display;
use std::io::Read;
use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, CallId, ClassificationInput, Classifier, Scope, Session,
    SessionId, Verdict, materialise_pending_scope, prepare_next_scope, process_action,
};
use file_logger::FileLogger;
use file_scope_store::{FilePendingScopeStore, FileScopeStore};
use osascript_prompter::OsascriptPrompter;

#[cfg(test)]
mod tests;

#[derive(serde::Deserialize)]
struct PreToolUsePayload {
    session_id: String,
    tool_use_id: Option<String>,
    /// Set only for in-process subagents — collapsed into `Session::id` here.
    agent_id: Option<String>,
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
    let raw: serde_json::Value = fail_closed("parse hook payload", serde_json::from_str(&buf));

    let event = raw
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    logger.log_line(&format!("hook-event: {event}"));

    let scopes = FileScopeStore::new(scope_dir);
    let pending = FilePendingScopeStore::new(pending_dir);

    match event.as_str() {
        "PreToolUse" => {
            let p: PreToolUsePayload = fail_closed("parse PreToolUse", serde_json::from_value(raw));
            handle_pre_tool_use(p, &cfg, &scopes, &pending, &logger);
        }
        "SubagentStart" => {
            if let Ok(p) = serde_json::from_value::<SubagentStartPayload>(raw) {
                let parent = SessionId::try_new(p.session_id).expect("valid parent id");
                let child = SessionId::try_new(p.agent_id).expect("valid agent_id");
                materialise(&scopes, &pending, &parent, &child, "SubagentStart", &logger);
            }
        }
        "SessionStart" => {
            if let Ok(p) = serde_json::from_value::<SessionStartPayload>(raw) {
                let child = SessionId::try_new(p.session_id).expect("valid child session id");
                if let Some(parent) = parent_from_env(&child) {
                    materialise(&scopes, &pending, &parent, &child, "SessionStart", &logger);
                }
                // No env var → root session. Nothing to materialise; root prompts via PreToolUse.
            }
        }
        _ => {} // Hook is registered for events we don't act on; ignore.
    }
}

fn handle_pre_tool_use(
    payload: PreToolUsePayload,
    cfg: &remit_config::RemitConfig,
    scopes: &FileScopeStore,
    pending: &FilePendingScopeStore,
    logger: &FileLogger,
) {
    let payload_session = SessionId::try_new(payload.session_id).expect("valid session id");

    // Sentinel: parent staging a scope for its next-spawned child. Intercept
    // before classification so the bash itself just runs as a no-op CLI call.
    if payload.tool_name == "Bash" {
        let cmd = payload
            .tool_input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if let Some(text) = parse_prepare_sentinel(cmd) {
            stage_pending(&payload_session, text, pending, logger);
            return;
        }
    }

    // Identity collapse: in-process subagents have a Claude-minted `agent_id`
    // that uniquely names *them*. We file scopes under it; the payload session
    // id becomes the parent for inheritance walks.
    let session = match payload.agent_id {
        Some(aid) => Session {
            id: SessionId::try_new(aid).expect("valid agent_id"),
            parent_id: Some(payload_session),
        },
        None => Session {
            parent_id: parent_from_env(&payload_session),
            id: payload_session,
        },
    };

    let input = ClassificationInput {
        session,
        action: Action {
            call_id: payload.tool_use_id.map(CallId::new),
            kind: ActionKind::new(payload.tool_name),
            args: ActionArgs::new(payload.tool_input.to_string()),
        },
        working_dir: payload.cwd,
    };

    let prompter = OsascriptPrompter::new();
    let classifier = Classifier::new(logger.clone());
    let verdict = process_action(&input, scopes, &prompter, scopes, &classifier);

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

fn stage_pending(
    parent: &SessionId,
    scope_text: String,
    pending: &FilePendingScopeStore,
    logger: &FileLogger,
) {
    match prepare_next_scope(pending, parent, Scope::new(&scope_text)) {
        Ok(None) => logger.log_line(&format!(
            "prepare-pending: parent={parent} scope={scope_text:?}"
        )),
        Ok(Some(prior)) => logger.log_line(&format!(
            "prepare-pending: parent={parent} scope={scope_text:?} (overwrote previous={:?})",
            prior.as_ref()
        )),
        Err(e) => {
            // Fail-closed: deny the bash so the agent doesn't proceed thinking the scope was set.
            logger.log_line(&format!(
                "prepare-pending: parent={parent} write-failed: {e}"
            ));
            std::process::exit(2);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn materialise(
    scopes: &FileScopeStore,
    pending: &FilePendingScopeStore,
    parent: &SessionId,
    child: &SessionId,
    hook: &str,
    logger: &FileLogger,
) {
    let outcome = match materialise_pending_scope(pending, scopes, parent, child) {
        Ok(Some(scope)) => format!("hit scope={:?}", scope.as_ref()),
        Ok(None) => "miss (no pending row; child will be blocked at first tool call)".into(),
        Err(e) => format!("error {e}"),
    };
    logger.log_line(&format!(
        "materialise: hook={hook} parent={parent} child={child} {outcome}"
    ));
}

fn parent_from_env(self_id: &SessionId) -> Option<SessionId> {
    parent_from_env_value(std::env::var("CLAUDE_SESSION_ID").ok(), self_id)
}

fn parent_from_env_value(env: Option<String>, self_id: &SessionId) -> Option<SessionId> {
    env.and_then(|s| SessionId::try_new(s).ok())
        .filter(|p| p != self_id)
}

/// Recognises `remit prepare-next-scope <text>` where `<text>` is `"…"`, `'…'`,
/// or a single bare word. Returns the unquoted text on match.
fn parse_prepare_sentinel(command: &str) -> Option<String> {
    const PREFIX: &str = "remit prepare-next-scope";
    let arg = command.trim_start().strip_prefix(PREFIX)?.trim_start();
    if arg.is_empty() {
        return None;
    }
    for quote in ['"', '\''] {
        if let Some(rest) = arg.strip_prefix(quote) {
            return rest.find(quote).map(|end| rest[..end].to_string());
        }
    }
    let end = arg
        .find(|c: char| c.is_whitespace() || c == ';' || c == '&' || c == '|')
        .unwrap_or(arg.len());
    Some(arg[..end].to_string())
}

fn fail_closed<T, E: Display>(ctx: &str, result: Result<T, E>) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("remit: {ctx}: {e}; deny");
        std::process::exit(2);
    })
}
