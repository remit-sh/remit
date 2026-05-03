use std::io::Read;
use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, CallId, ClassificationInput, Classifier, Decision, Session,
    SessionId, SpawnId,
};
use file_logger::FileLogger;

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
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .expect("read stdin");
    let payload: HookPayload = serde_json::from_str(&buf).expect("parse hook payload");

    let session_id = SessionId::new(payload.session_id);
    
    // Takes parent from env if present
    let parent_id = infer_parent_id(
        std::env::var("CLAUDE_SESSION_ID").ok().map(SessionId::new),
        &session_id,
    );

    let input = ClassificationInput {
        session: Session {
            id: session_id,
            spawn_id: payload.agent_id.map(SpawnId::new),
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
    let classifier = Classifier::new(FileLogger::new(log_path));

    match classifier.classify(&input) {
        Decision::Green => {}
        Decision::Red { reason } => {
            eprintln!("{reason}");
            std::process::exit(2);
        }
    }
}

fn infer_parent_id(env_value: Option<SessionId>, session_id: &SessionId) -> Option<SessionId> {
    env_value.filter(|p| p != session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_env_means_no_parent() {
        assert_eq!(
            infer_parent_id(None, &SessionId::new("self".to_string())),
            None
        );
    }

    #[test]
    fn env_differing_from_self_is_parent() {
        let env = SessionId::new("parent-id".to_string());
        let me = SessionId::new("self".to_string());
        assert_eq!(infer_parent_id(Some(env.clone()), &me), Some(env));
    }

    #[test]
    fn env_equal_to_self_is_ignored() {
        let me = SessionId::new("self".to_string());
        assert_eq!(infer_parent_id(Some(me.clone()), &me), None);
    }
}
