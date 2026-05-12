//! Live integration test that pops a real macOS dialog. Opt-in only:
//!
//!   cargo test -p osascript-prompter -- --ignored
//!
//! The test asks the user to type a known scope. CI never runs it.

#![cfg(target_os = "macos")]

use std::path::Path;

use domain::Prompter;
use osascript_prompter::OsascriptPrompter;

#[test]
#[ignore = "interactive: pops a real osascript dialog"]
fn live_dialog_returns_typed_scope() {
    let prompter = OsascriptPrompter::new();
    let scope = prompter
        .update_scope(None, Path::new("/tmp/integration-test"))
        .expect("dialog should succeed");
    println!("captured scope: {}", scope.as_ref());
    assert!(!scope.as_ref().is_empty());
}
