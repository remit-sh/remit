//! End-to-end plumbing tests for [`SwiftPrompter`]: fake `remit-prompter`
//! scripts on disk verify request bytes, response parsing, exit codes,
//! timeouts, and size caps without spinning up real AppKit UI.
//!
//! The fake binaries are short shell scripts written to a tempdir per test;
//! [`SwiftPrompter::with_binary`] points the adapter at them.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use domain::{Action, ActionArgs, ActionKind, PromptError, Prompter, Scope, VerdictChoice};
use swift_prompter::SwiftPrompter;

/// Write `script` to `<tempdir>/fake-prompter` with executable perms. Returns
/// (path-to-binary, tempdir-handle so it isn't dropped early).
fn fake_binary(script: &str) -> (PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("fake-prompter");
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write script");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    (path, dir)
}

fn prompter_with(script: &str) -> (SwiftPrompter, tempfile::TempDir) {
    let (path, dir) = fake_binary(script);
    let p = SwiftPrompter::new()
        .with_binary(path)
        .with_timeout(Duration::from_secs(30));
    (p, dir)
}

fn scope(s: &str) -> Scope {
    Scope::try_new(s.to_string()).expect("valid scope")
}

fn action(kind: &str, args: &str) -> Action {
    Action {
        call_id: None,
        kind: ActionKind::new(kind.to_string()),
        args: ActionArgs::new(args.to_string()),
    }
}

fn bash_action() -> Action {
    action("Bash", r#"{"command":"echo hi"}"#)
}

// ----- choose: every response variant round-trips -----

#[test]
fn choose_returns_deny() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"choose","choice":"deny"}'"#);
    let v = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap();
    assert_eq!(v, VerdictChoice::Deny);
}

#[test]
fn choose_returns_allow() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"choose","choice":"allow"}'"#);
    let v = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap();
    assert_eq!(v, VerdictChoice::Allow);
}

#[test]
fn choose_returns_update_scope() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"choose","choice":"update_scope"}'"#);
    let v = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap();
    assert_eq!(v, VerdictChoice::UpdateScope);
}

#[test]
fn choose_returns_disable_session() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"choose","choice":"disable_session"}'"#);
    let v = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap();
    assert_eq!(v, VerdictChoice::DisableSession);
}

#[test]
fn choose_cancelled_maps_to_prompt_error_cancelled() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"choose","cancelled":true}'"#);
    let err = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap_err();
    assert!(matches!(err, PromptError::Cancelled));
}

#[test]
fn choose_unknown_choice_label_is_unavailable() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"choose","choice":"nonsense"}'"#);
    let err = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap_err();
    assert!(matches!(err, PromptError::Unavailable(_)));
}

// ----- update_scope -----

#[test]
fn update_scope_returns_scope() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"update_scope","scope":"new scope"}'"#);
    let s = p.update_scope(None, Path::new("/w")).unwrap();
    assert_eq!(s.as_ref(), "new scope");
}

#[test]
fn update_scope_cancelled_maps_to_prompt_error_cancelled() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"update_scope","cancelled":true}'"#);
    let err = p.update_scope(None, Path::new("/w")).unwrap_err();
    assert!(matches!(err, PromptError::Cancelled));
}

// ----- confirm_disable -----

#[test]
fn confirm_disable_true() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"confirm_disable","confirmed":true}'"#);
    assert!(p.confirm_disable().unwrap());
}

#[test]
fn confirm_disable_false() {
    let (p, _d) = prompter_with(r#"echo '{"kind":"confirm_disable","confirmed":false}'"#);
    assert!(!p.confirm_disable().unwrap());
}

// ----- request shape: capture stdin via fake that echoes its input -----

#[test]
fn request_carries_protocol_kind_and_fields() {
    let dir = tempfile::tempdir().unwrap();
    let captured = dir.path().join("captured.json");
    let captured_str = captured.display().to_string();
    let script = format!(
        r#"cat > {captured} && echo '{{"kind":"choose","choice":"deny"}}'"#,
        captured = shell_escape(&captured_str),
    );
    let (path, _d) = fake_binary(&script);
    let p = SwiftPrompter::new()
        .with_binary(path)
        .with_timeout(Duration::from_secs(30));
    let _ = p
        .choose(
            &scope("my scope"),
            &action("Bash", r#"{"command":"rm -rf /tmp/foo"}"#),
            "out of bounds",
            Path::new("/some/path"),
        )
        .unwrap();

    let body = fs::read_to_string(&captured).unwrap();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["protocol"], 1);
    assert_eq!(v["kind"], "choose");
    assert_eq!(v["scope"], "my scope");
    assert_eq!(v["reason"], "out of bounds");
    assert_eq!(v["working_dir"], "/some/path");
    assert_eq!(v["tool_name"], "Bash");
    assert_eq!(v["args_preview"], "command: rm -rf /tmp/foo");
}

// ----- failure modes -----

#[test]
fn non_zero_exit_maps_to_unavailable_with_stderr() {
    let (p, _d) = prompter_with(
        r#"echo "boom" >&2
exit 2"#,
    );
    let err = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap_err();
    let PromptError::Unavailable(boxed) = err else {
        panic!("expected Unavailable");
    };
    let msg = format!("{boxed}");
    assert!(
        msg.contains("boom"),
        "stderr 'boom' should be in error: {msg}"
    );
}

#[test]
fn stdout_is_discarded_on_non_zero_exit() {
    // Binary emits a "scope" on stdout while failing. The error chain must
    // NOT include that scope text — would leak user input into logs.
    let (p, _d) = prompter_with(
        r#"echo '{"kind":"update_scope","scope":"SECRET_SCOPE_NEVER_LEAK"}'
echo "operator error" >&2
exit 1"#,
    );
    let err = p.update_scope(None, Path::new("/w")).unwrap_err();
    let PromptError::Unavailable(boxed) = err else {
        panic!("expected Unavailable");
    };
    let msg = format!("{boxed}");
    assert!(
        !msg.contains("SECRET_SCOPE_NEVER_LEAK"),
        "stdout MUST NOT appear in error chain: {msg}"
    );
    assert!(msg.contains("operator error"));
}

#[test]
fn binary_missing_maps_to_unavailable() {
    let p = SwiftPrompter::new()
        .with_binary(PathBuf::from("/definitely/does/not/exist/remit-prompter"))
        .with_timeout(Duration::from_secs(30));
    let err = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap_err();
    assert!(matches!(err, PromptError::Unavailable(_)));
}

#[test]
fn hung_child_is_killed_after_timeout() {
    let (path, _d) = fake_binary("sleep 30");
    let p = SwiftPrompter::new()
        .with_binary(path)
        .with_timeout(Duration::from_millis(300));
    let start = std::time::Instant::now();
    let err = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap_err();
    let elapsed = start.elapsed();
    assert!(matches!(err, PromptError::Unavailable(_)));
    assert!(
        elapsed < Duration::from_secs(5),
        "timeout should fire quickly, took {elapsed:?}"
    );
}

#[test]
fn malformed_json_response_is_unavailable() {
    let (p, _d) = prompter_with(r#"echo 'not json at all'"#);
    let err = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap_err();
    assert!(matches!(err, PromptError::Unavailable(_)));
}

#[test]
fn response_kind_mismatch_is_unavailable() {
    // Binary returns a confirm_disable kind tag to a choose request.
    let (p, _d) = prompter_with(r#"echo '{"kind":"confirm_disable","choice":"allow"}'"#);
    let err = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap_err();
    let PromptError::Unavailable(e) = err else {
        panic!("expected Unavailable");
    };
    let msg = format!("{e}");
    assert!(
        msg.contains("kind mismatch"),
        "kind mismatch should surface: {msg}"
    );
}

#[test]
fn response_missing_kind_is_unavailable() {
    // Untagged response (legacy shape) is rejected; kind is mandatory.
    let (p, _d) = prompter_with(r#"echo '{"choice":"allow"}'"#);
    let err = p
        .choose(&scope("s"), &bash_action(), "r", Path::new("/w"))
        .unwrap_err();
    assert!(matches!(err, PromptError::Unavailable(_)));
}

#[test]
fn args_preview_never_appears_in_error_chain() {
    // The action args / args_preview / tool_name make it to stdin but must
    // never appear on the error path — they may contain sensitive content
    // (paths the user is reviewing, command strings, etc.). Forces a
    // non-zero exit so the adapter's Unavailable mapping runs, then asserts
    // the marker strings are absent.
    let (p, _d) = prompter_with(
        r#"echo "operator error" >&2
exit 1"#,
    );
    let secret_marker = "ARGS_PREVIEW_SECRET_MARKER_DO_NOT_LEAK";
    let tool_marker = "TOOL_NAME_SECRET_MARKER_DO_NOT_LEAK";
    let args = format!(r#"{{"command":"{secret_marker}"}}"#);
    let act = action(tool_marker, &args);
    let err = p
        .choose(&scope("s"), &act, "r", Path::new("/w"))
        .unwrap_err();
    let PromptError::Unavailable(boxed) = err else {
        panic!("expected Unavailable");
    };
    // Walk the source chain too — defence against future code routing leaks
    // via `#[source]`.
    let mut msg = format!("{boxed}");
    let mut current = boxed.source();
    while let Some(c) = current {
        msg.push('\n');
        msg.push_str(&c.to_string());
        current = c.source();
    }
    assert!(
        !msg.contains(secret_marker),
        "args_preview leaked to error chain: {msg}"
    );
    assert!(
        !msg.contains(tool_marker),
        "tool_name leaked to error chain: {msg}"
    );
}

/// Quote a path for safe shell embedding. Only handles paths without single
/// quotes (sufficient for tempdir paths macOS / Linux produce).
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
