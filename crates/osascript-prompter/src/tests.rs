use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};

use domain::PromptError;

use super::*;

fn output(status_raw: i32, stdout: &str, stderr: &str) -> Output {
    Output {
        status: ExitStatus::from_raw(status_raw),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn parses_typed_scope_from_success_output() {
    let out = output(
        0,
        "button returned:Set scope, text returned:do the thing\n",
        "",
    );
    let scope = parse_osascript_output(&out).unwrap();
    assert_eq!(scope.as_ref(), "do the thing");
}

#[test]
fn empty_answer_is_treated_as_cancelled() {
    let out = output(0, "button returned:Set scope, text returned:\n", "");
    assert!(matches!(
        parse_osascript_output(&out),
        Err(PromptError::Cancelled)
    ));
}

#[test]
fn whitespace_only_answer_is_treated_as_cancelled() {
    let out = output(0, "button returned:Set scope, text returned:   \n", "");
    assert!(matches!(
        parse_osascript_output(&out),
        Err(PromptError::Cancelled)
    ));
}

#[test]
fn user_canceled_stderr_is_cancelled() {
    // osascript signals cancel via non-zero exit + a `User canceled` stderr
    let out = output(1 << 8, "", "0:0: execution error: User canceled. (-128)\n");
    assert!(matches!(
        parse_osascript_output(&out),
        Err(PromptError::Cancelled)
    ));
}

#[test]
fn other_failure_is_unavailable() {
    let out = output(2 << 8, "", "0:0: execution error: oh no (-1)\n");
    let err = parse_osascript_output(&out).unwrap_err();
    assert!(matches!(err, PromptError::Unavailable(_)));
    assert!(format!("{err:?}").contains("osascript"));
}

#[test]
fn missing_text_returned_marker_is_unavailable() {
    let out = output(0, "weird output without the marker\n", "");
    assert!(matches!(
        parse_osascript_output(&out),
        Err(PromptError::Unavailable(_))
    ));
}

#[test]
fn answer_with_embedded_comma_is_preserved() {
    let out = output(
        0,
        "button returned:Set scope, text returned:fix bug, then deploy\n",
        "",
    );
    let scope = parse_osascript_output(&out).unwrap();
    assert_eq!(scope.as_ref(), "fix bug, then deploy");
}
