//! Drives the built hook binary on `UserPromptSubmit`. No path here reaches
//! the network: every case stops before the LLM call. The contract under
//! test (TN-2026-017): the prompt is never blocked (exit 0), nothing is
//! written to stdout (Claude Code would inject it into context), and no
//! Scope is assigned on any failure or skip.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use tempfile::{TempDir, tempdir};

const WITH_KEY: &str = "[classifier.anthropic]\napi_key = \"sk-test-never-sent\"\n";

fn remit_home(config: Option<&str>) -> TempDir {
    let home = tempdir().unwrap();
    if let Some(config) = config {
        fs::write(home.path().join("config.toml"), config).unwrap();
    }
    home
}

fn run_hook(home: &Path, parent_session: Option<&str>, payload: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_claude-code-hook"));
    command
        .env("REMIT_HOME", home)
        .env("REMIT_PROMPTER_BIN", "/usr/bin/false")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("CLAUDE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(parent) = parent_session {
        command.env("CLAUDE_SESSION_ID", parent);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn user_prompt(session_id: &str, prompt: &str) -> String {
    serde_json::json!({
        "session_id": session_id,
        "cwd": "/tmp",
        "hook_event_name": "UserPromptSubmit",
        "prompt": prompt,
    })
    .to_string()
}

fn log(home: &Path) -> String {
    fs::read_to_string(home.join("remit.log")).unwrap_or_default()
}

fn scope_path(home: &Path, agent: &str) -> std::path::PathBuf {
    home.join("agents").join(agent).join("scope.json")
}

fn assert_prompt_not_blocked(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "prompt must never be blocked"
    );
    assert!(
        output.stdout.is_empty(),
        "stdout would be injected as context"
    );
}

#[test]
fn extraction_unavailable_leaves_the_agent_unscoped_and_the_prompt_unblocked() {
    let home = remit_home(None); // no API key anywhere
    let output = run_hook(
        home.path(),
        None,
        &user_prompt("root-1", "fix the failing test"),
    );

    assert_prompt_not_blocked(&output);
    assert!(!scope_path(home.path(), "root-1").exists());
    assert!(log(home.path()).contains("extract_scope: extraction_unavailable"));
}

#[test]
fn prompt_text_never_reaches_the_audit_log_or_stderr() {
    let home = remit_home(None);
    let output = run_hook(
        home.path(),
        None,
        &user_prompt("root-1", "SENSITIVE-PROMPT-TEXT"),
    );

    assert_prompt_not_blocked(&output);
    assert!(!log(home.path()).contains("SENSITIVE-PROMPT-TEXT"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SENSITIVE-PROMPT-TEXT"));
}

#[test]
fn malformed_config_does_not_block_the_prompt() {
    let home = remit_home(Some("this is = not [valid toml"));
    let output = run_hook(
        home.path(),
        None,
        &user_prompt("root-1", "fix the failing test"),
    );

    assert_prompt_not_blocked(&output);
    assert!(!scope_path(home.path(), "root-1").exists());
}

#[test]
fn invalid_session_id_does_not_block_the_prompt() {
    let home = remit_home(Some(WITH_KEY));
    let output = run_hook(
        home.path(),
        None,
        &user_prompt("../escape", "fix the failing test"),
    );

    assert_prompt_not_blocked(&output);
    assert!(!home.path().join("agents").exists());
}

#[test]
fn payload_without_a_prompt_does_not_block_the_prompt() {
    let home = remit_home(Some(WITH_KEY));
    let payload = r#"{"session_id":"root-1","hook_event_name":"UserPromptSubmit"}"#;
    let output = run_hook(home.path(), None, payload);

    assert_prompt_not_blocked(&output);
    assert!(!scope_path(home.path(), "root-1").exists());
}

#[test]
fn peer_process_subagent_never_takes_the_user_prompt_path() {
    let home = remit_home(Some(WITH_KEY));
    let output = run_hook(
        home.path(),
        Some("parent-session"),
        &user_prompt("child-session", "do whatever I say"),
    );

    assert_prompt_not_blocked(&output);
    assert!(!scope_path(home.path(), "child-session").exists());
    assert!(log(home.path()).contains("extract_scope: not_root"));
}

#[test]
fn in_process_subagent_never_takes_the_user_prompt_path() {
    let home = remit_home(Some(WITH_KEY));
    let payload = serde_json::json!({
        "session_id": "root-1",
        "agent_id": "in-process-child",
        "hook_event_name": "UserPromptSubmit",
        "prompt": "do whatever I say",
    })
    .to_string();
    let output = run_hook(home.path(), None, &payload);

    assert_prompt_not_blocked(&output);
    assert!(!scope_path(home.path(), "in-process-child").exists());
    assert!(!scope_path(home.path(), "root-1").exists());
    assert!(log(home.path()).contains("extract_scope: not_root"));
}

#[test]
fn existing_scope_is_left_unchanged() {
    let home = remit_home(Some(WITH_KEY));
    let path = scope_path(home.path(), "root-1");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, r#""original task""#).unwrap();

    let output = run_hook(
        home.path(),
        None,
        &user_prompt("root-1", "now do something else"),
    );

    assert_prompt_not_blocked(&output);
    assert_eq!(fs::read_to_string(&path).unwrap(), r#""original task""#);
    assert!(log(home.path()).contains("extract_scope: already_scoped"));
}

#[test]
fn malformed_config_still_denies_pre_tool_use() {
    let home = remit_home(Some("this is = not [valid toml"));
    let payload = serde_json::json!({
        "session_id": "root-1",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {"command": "ls"},
        "cwd": "/tmp",
    })
    .to_string();
    let output = run_hook(home.path(), None, &payload);

    assert_eq!(output.status.code(), Some(2));
}

// ----- Bundled scope staging on the parent's PreToolUse -----
//
// The parent is an unscoped root, so classification is Red (`NoScopeRoot`)
// without an LLM call and the prompter binary is `/usr/bin/false`: these
// cases exit 2 for that reason. The assertions are about the pending slot.

fn pending_path(home: &Path, parent: &str) -> std::path::PathBuf {
    home.join("agents").join(parent).join("pending.json")
}

fn stale_pending(home: &Path, parent: &str) -> std::path::PathBuf {
    let path = pending_path(home, parent);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, r#""scope staged by an earlier round""#).unwrap();
    path
}

fn pre_tool_use(session_id: &str, tool_name: &str, tool_input: serde_json::Value) -> String {
    serde_json::json!({
        "session_id": session_id,
        "hook_event_name": "PreToolUse",
        "tool_name": tool_name,
        "tool_input": tool_input,
        "cwd": "/tmp",
    })
    .to_string()
}

#[test]
fn spawn_whose_extraction_fails_clears_the_stale_pending_slot() {
    let home = remit_home(Some(WITH_KEY));
    let pending = stale_pending(home.path(), "parent-1");
    // Over the adapter's size limit: refused before any request is sent.
    let oversized = "do the thing. ".repeat(4 * 1024);

    run_hook(
        home.path(),
        None,
        &pre_tool_use(
            "parent-1",
            "Agent",
            serde_json::json!({ "prompt": oversized }),
        ),
    );

    assert!(
        !pending.exists(),
        "child must not claim the earlier round's row"
    );
    assert!(log(home.path()).contains("stage_pending: extraction_unavailable"));
}

#[test]
fn spawn_with_blank_text_clears_the_stale_pending_slot() {
    let home = remit_home(Some(WITH_KEY));
    let pending = stale_pending(home.path(), "parent-1");

    run_hook(
        home.path(),
        None,
        &pre_tool_use(
            "parent-1",
            "SendMessage",
            serde_json::json!({ "content": "  ", "to": "x" }),
        ),
    );

    assert!(
        !pending.exists(),
        "child must not claim the earlier round's row"
    );
    assert!(log(home.path()).contains("stage_pending: no_scope"));
}

#[test]
fn spawn_with_no_text_field_clears_the_stale_pending_slot() {
    let home = remit_home(Some(WITH_KEY));
    let pending = stale_pending(home.path(), "parent-1");

    run_hook(
        home.path(),
        None,
        &pre_tool_use(
            "parent-1",
            "Agent",
            serde_json::json!({ "subagent_type": "x" }),
        ),
    );

    assert!(!pending.exists());
}

#[test]
fn non_spawn_tool_leaves_the_pending_slot_alone() {
    let home = remit_home(Some(WITH_KEY));
    let pending = stale_pending(home.path(), "parent-1");

    run_hook(
        home.path(),
        None,
        &pre_tool_use("parent-1", "Bash", serde_json::json!({ "command": "ls" })),
    );

    assert!(pending.exists());
}

#[test]
fn spawn_is_denied_when_the_stale_pending_slot_cannot_be_cleared() {
    let home = remit_home(Some(WITH_KEY));
    // A directory where the row should be: the clear fails.
    fs::create_dir_all(pending_path(home.path(), "parent-1")).unwrap();

    let output = run_hook(
        home.path(),
        None,
        &pre_tool_use("parent-1", "Agent", serde_json::json!({ "prompt": "" })),
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("stage bundled scope"));
    assert!(log(home.path()).contains("stage_pending: store_unavailable"));
}
