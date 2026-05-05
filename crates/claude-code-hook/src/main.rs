use std::fmt::Display;
use std::io::Read;
use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, CallId, ClassificationInput, Classifier, Session, SessionId,
    SubagentId, Verdict, process_action,
};
use file_logger::FileLogger;
use file_scope_store::FileScopeStore;
use osascript_prompter::OsascriptPrompter;

#[derive(serde::Deserialize)]
struct HookPayload {
    session_id: String,
    tool_use_id: Option<String>,
    agent_id: Option<String>,
    tool_name: String,
    tool_input: serde_json::Value,
    cwd: PathBuf,
}

fn main() {
    let cfg = fail_closed("config", remit_config::load());
    let scope_dir = fail_closed("scope dir", remit_config::scope_dir());

    let mut buf = String::new();
    fail_closed("read stdin", std::io::stdin().read_to_string(&mut buf));
    let payload: HookPayload = fail_closed("parse hook payload", serde_json::from_str(&buf));

    let session_id = fail_closed("session id", SessionId::try_new(payload.session_id));
    let parent_id = infer_parent_id(
        std::env::var("CLAUDE_SESSION_ID")
            .ok()
            .and_then(|s| SessionId::try_new(s).ok()),
        &session_id,
    );

    let input = ClassificationInput {
        session: Session {
            id: session_id,
            subagent_id: payload.agent_id.map(SubagentId::new),
            parent_id,
        },
        action: Action {
            call_id: payload.tool_use_id.map(CallId::new),
            kind: ActionKind::new(payload.tool_name),
            args: ActionArgs::new(payload.tool_input.to_string()),
        },
        working_dir: payload.cwd,
    };

    let log_path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../remit.log"));
    let store = FileScopeStore::new(scope_dir);
    let prompter = OsascriptPrompter::new();
    let classifier = Classifier::new(FileLogger::new(log_path));

    let verdict = process_action(&input, &store, &prompter, &store, &classifier);

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

fn infer_parent_id(env_value: Option<SessionId>, session_id: &SessionId) -> Option<SessionId> {
    env_value.filter(|p| p != session_id)
}

fn fail_closed<T, E: Display>(ctx: &str, result: Result<T, E>) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("remit: {ctx}: {e}; deny");
        std::process::exit(2);
    })
}

#[cfg(test)]
mod tests;
