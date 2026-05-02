use std::io::Read;
use std::path::PathBuf;

use domain::{Action, ClassificationInput, Classifier, Decision, Session};
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

    let parent_id = std::env::var("CLAUDE_SESSION_ID")
        .ok()
        .filter(|p| p != &payload.session_id);

    let input = ClassificationInput {
        session: Session {
            id: payload.session_id,
            spawn_id: payload.agent_id,
            parent_id,
        },
        action: Action {
            call_id: payload.tool_use_id,
            kind: payload.tool_name,
            args: payload.tool_input.to_string(),
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
