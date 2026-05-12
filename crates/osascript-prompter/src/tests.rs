use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

use domain::{PromptError, VerdictChoice};

use super::*;

fn output(status_raw: i32, stdout: &str, stderr: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(status_raw),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn choose_parses_deny_selection() {
    let out = output(0, "Deny\n", "");
    assert_eq!(parse_choose_output(&out).unwrap(), VerdictChoice::Deny);
}

#[test]
fn choose_parses_allow_selection() {
    let out = output(0, "Allow\n", "");
    assert_eq!(parse_choose_output(&out).unwrap(), VerdictChoice::Allow);
}

#[test]
fn choose_parses_update_scope_selection() {
    let out = output(0, "Update scope\n", "");
    assert_eq!(
        parse_choose_output(&out).unwrap(),
        VerdictChoice::UpdateScope
    );
}

#[test]
fn choose_parses_disable_session_selection() {
    let out = output(0, "Disable session\n", "");
    assert_eq!(
        parse_choose_output(&out).unwrap(),
        VerdictChoice::DisableSession
    );
}

#[test]
fn choose_false_output_is_cancelled() {
    // `choose from list` prints the literal `false` when the user cancels.
    let out = output(0, "false\n", "");
    assert!(matches!(
        parse_choose_output(&out),
        Err(PromptError::Cancelled)
    ));
}

#[test]
fn choose_user_cancel_via_stderr_is_cancelled() {
    let out = output(1 << 8, "", "0:0: execution error: User canceled. (-128)\n");
    assert!(matches!(
        parse_choose_output(&out),
        Err(PromptError::Cancelled)
    ));
}

#[test]
fn choose_unknown_selection_is_unavailable() {
    let out = output(0, "Whatever\n", "");
    assert!(matches!(
        parse_choose_output(&out),
        Err(PromptError::Unavailable(_))
    ));
}

#[test]
fn choose_other_failure_is_unavailable() {
    let out = output(2 << 8, "", "0:0: execution error: oh no (-1)\n");
    assert!(matches!(
        parse_choose_output(&out),
        Err(PromptError::Unavailable(_))
    ));
}

#[test]
fn update_scope_parses_typed_scope() {
    let out = output(0, "button returned:Save, text returned:do the thing\n", "");
    let scope = parse_update_scope_output(&out).unwrap();
    assert_eq!(scope.as_ref(), "do the thing");
}

#[test]
fn update_scope_empty_answer_is_cancelled() {
    let out = output(0, "button returned:Save, text returned:\n", "");
    assert!(matches!(
        parse_update_scope_output(&out),
        Err(PromptError::Cancelled)
    ));
}

#[test]
fn update_scope_whitespace_only_is_cancelled() {
    let out = output(0, "button returned:Save, text returned:   \n", "");
    assert!(matches!(
        parse_update_scope_output(&out),
        Err(PromptError::Cancelled)
    ));
}

#[test]
fn update_scope_user_cancel_is_cancelled() {
    let out = output(1 << 8, "", "0:0: execution error: User canceled. (-128)\n");
    assert!(matches!(
        parse_update_scope_output(&out),
        Err(PromptError::Cancelled)
    ));
}

#[test]
fn update_scope_other_failure_is_unavailable() {
    let out = output(2 << 8, "", "0:0: execution error: oh no (-1)\n");
    let err = parse_update_scope_output(&out).unwrap_err();
    assert!(matches!(err, PromptError::Unavailable(_)));
}

#[test]
fn update_scope_missing_text_marker_is_unavailable() {
    let out = output(0, "weird output without the marker\n", "");
    assert!(matches!(
        parse_update_scope_output(&out),
        Err(PromptError::Unavailable(_))
    ));
}

#[test]
fn update_scope_does_not_leak_typed_text_into_error_chain() {
    // A successful osascript run that doesn't carry the expected `text returned:`
    // marker still has the user's typed scope sitting in stdout. The error chain
    // is persisted by `log_error` — it must never contain that text.
    let scope_text = "secret scope content";
    let out = output(0, scope_text, "");
    let err = parse_update_scope_output(&out).unwrap_err();
    let PromptError::Unavailable(boxed) = err else {
        panic!("expected Unavailable");
    };
    let mut chain = boxed.to_string();
    let mut current = boxed.source();
    while let Some(c) = current {
        chain.push_str(": ");
        chain.push_str(&c.to_string());
        current = c.source();
    }
    assert!(
        !chain.contains(scope_text),
        "scope text leaked into error chain: {chain}"
    );
}

#[test]
fn update_scope_answer_with_embedded_comma_is_preserved() {
    let out = output(
        0,
        "button returned:Save, text returned:fix bug, then deploy\n",
        "",
    );
    let scope = parse_update_scope_output(&out).unwrap();
    assert_eq!(scope.as_ref(), "fix bug, then deploy");
}

#[test]
fn confirm_disable_returns_true_on_disable_button() {
    let out = output(0, "button returned:Disable\n", "");
    assert!(parse_confirm_disable_output(&out).unwrap());
}

#[test]
fn confirm_disable_returns_false_on_cancel_button() {
    let out = output(0, "button returned:Cancel\n", "");
    assert!(!parse_confirm_disable_output(&out).unwrap());
}

#[test]
fn confirm_disable_returns_false_on_user_cancel() {
    let out = output(1 << 8, "", "0:0: execution error: User canceled. (-128)\n");
    assert!(!parse_confirm_disable_output(&out).unwrap());
}

#[test]
fn confirm_disable_other_failure_is_unavailable() {
    let out = output(2 << 8, "", "0:0: execution error: boom (-1)\n");
    assert!(matches!(
        parse_confirm_disable_output(&out),
        Err(PromptError::Unavailable(_))
    ));
}
