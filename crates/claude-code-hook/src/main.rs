use std::io::Read;
use std::path::PathBuf;

use domain::{ClassificationInput, Decision, classify};
use file_logger::FileLogger;

#[derive(serde::Deserialize)]
struct HookPayload {
    session_id: String,
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

    let input = ClassificationInput {
        session_id: payload.session_id,
        action_kind: payload.tool_name,
        action: payload.tool_input.to_string(),
        working_dir: payload.cwd,
    };

    let log_path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../remit.log"));
    let logger = FileLogger::new(log_path);

    match classify(&input, &logger) {
        Decision::Green => {}
        Decision::Red { reason } => {
            eprintln!("{reason}");
            std::process::exit(2);
        }
    }
}
