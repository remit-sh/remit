//! Live integration test that pops a real macOS dialog. Opt-in only:
//!
//!   cargo test -p osascript-prompter -- --ignored
//!
//! The test asks the user to type a known scope. CI never runs it.

#![cfg(target_os = "macos")]

use domain::{AgentId, ScopePrompter};
use osascript_prompter::OsascriptPrompter;

#[test]
#[ignore = "interactive: pops a real osascript dialog"]
fn live_dialog_returns_typed_scope() {
    let prompter = OsascriptPrompter::new();
    let agent_id = AgentId::try_new("integration-test").unwrap();
    let scope = prompter.prompt(&agent_id).expect("dialog should succeed");
    println!("captured scope: {}", scope.as_ref());
    assert!(!scope.as_ref().is_empty());
}
