//! Unit tests for response parsing and request construction. Process-level
//! plumbing (spawn / timeout / size caps) lives in `tests/integration.rs` where
//! a fake binary script gives us deterministic exit codes and stdout payloads.

use super::*;
use serde_json::json;

#[test]
fn choose_request_carries_protocol_version_and_kind() {
    let req = ChooseRequest {
        protocol: PROTOCOL_VERSION,
        kind: "choose",
        scope: "s",
        reason: "r",
        working_dir: "/w",
        tool_name: "Bash",
        args_preview: "ls -la",
    };
    let v: serde_json::Value = serde_json::from_slice(&serde_json::to_vec(&req).unwrap()).unwrap();
    assert_eq!(v["protocol"], 1);
    assert_eq!(v["kind"], "choose");
    assert_eq!(v["scope"], "s");
    assert_eq!(v["reason"], "r");
    assert_eq!(v["working_dir"], "/w");
    assert_eq!(v["tool_name"], "Bash");
    assert_eq!(v["args_preview"], "ls -la");
}

#[test]
fn update_scope_request_omits_current_when_first_time() {
    let req = UpdateScopeRequest {
        protocol: PROTOCOL_VERSION,
        kind: "update_scope",
        current: None,
        working_dir: "/w",
    };
    let v: serde_json::Value = serde_json::from_slice(&serde_json::to_vec(&req).unwrap()).unwrap();
    assert!(v.get("current").is_none());
}

#[test]
fn update_scope_request_includes_current_when_editing() {
    let req = UpdateScopeRequest {
        protocol: PROTOCOL_VERSION,
        kind: "update_scope",
        current: Some("existing"),
        working_dir: "/w",
    };
    let v: serde_json::Value = serde_json::from_slice(&serde_json::to_vec(&req).unwrap()).unwrap();
    assert_eq!(v["current"], "existing");
}

#[test]
fn confirm_disable_request_has_no_working_dir() {
    let req = ConfirmDisableRequest {
        protocol: PROTOCOL_VERSION,
        kind: "confirm_disable",
    };
    let v: serde_json::Value = serde_json::from_slice(&serde_json::to_vec(&req).unwrap()).unwrap();
    assert_eq!(v["kind"], "confirm_disable");
    assert!(v.get("working_dir").is_none());
}

#[test]
fn choose_response_parses_each_variant() {
    for label in ["deny", "allow", "update_scope", "disable_session"] {
        let raw = json!({"kind": "choose", "choice": label}).to_string();
        let r: ChooseResponse = serde_json::from_str(&raw).unwrap();
        assert_eq!(r.kind, "choose");
        assert_eq!(r.choice.as_deref(), Some(label));
        assert!(r.cancelled.is_none());
    }
}

#[test]
fn choose_response_parses_cancelled() {
    let r: ChooseResponse = serde_json::from_str(r#"{"kind":"choose","cancelled":true}"#).unwrap();
    assert_eq!(r.cancelled, Some(true));
    assert!(r.choice.is_none());
}

#[test]
fn update_scope_response_parses_scope() {
    let r: UpdateScopeResponse =
        serde_json::from_str(r#"{"kind":"update_scope","scope":"new scope"}"#).unwrap();
    assert_eq!(r.scope.as_deref(), Some("new scope"));
}

#[test]
fn update_scope_response_parses_cancelled() {
    let r: UpdateScopeResponse =
        serde_json::from_str(r#"{"kind":"update_scope","cancelled":true}"#).unwrap();
    assert_eq!(r.cancelled, Some(true));
}

#[test]
fn confirm_disable_response_parses_both_states() {
    let yes: ConfirmDisableResponse =
        serde_json::from_str(r#"{"kind":"confirm_disable","confirmed":true}"#).unwrap();
    assert!(yes.confirmed);
    let no: ConfirmDisableResponse =
        serde_json::from_str(r#"{"kind":"confirm_disable","confirmed":false}"#).unwrap();
    assert!(!no.confirmed);
}

#[test]
fn check_kind_accepts_match() {
    assert!(check_kind("choose", "choose").is_ok());
}

#[test]
fn check_kind_rejects_mismatch() {
    let err = check_kind("confirm_disable", "choose").unwrap_err();
    let PromptError::Unavailable(boxed) = err else {
        panic!("expected Unavailable");
    };
    let msg = format!("{boxed}");
    assert!(msg.contains("kind mismatch"));
    assert!(msg.contains("choose"));
    assert!(msg.contains("confirm_disable"));
}

#[test]
fn binary_path_prefers_explicit_override() {
    let prompter = SwiftPrompter::new().with_binary(PathBuf::from("/explicit/path"));
    assert_eq!(prompter.binary_path(), PathBuf::from("/explicit/path"));
}

#[test]
fn default_constructor_is_callable() {
    // Smoke: Default impl exists and doesn't panic.
    let _ = SwiftPrompter::default();
}

#[test]
fn surface_timeout_is_30s() {
    // Long surface timeouts double as user-visible freezes when the UI doesn't
    // come up; the constant is load-bearing for UX. See TN-2026-014. Past the
    // surface deadline (no READY marker, no exit) the child is killed.
    assert_eq!(SURFACE_TIMEOUT, Duration::from_secs(30));
}

// ----- args_preview helpers -----

#[test]
fn prettify_args_single_string_field() {
    assert_eq!(
        prettify_args(r#"{"command":"echo hi"}"#),
        "command: echo hi"
    );
}

#[test]
fn prettify_args_multiple_fields_joined_with_commas() {
    // serde_json::Map is BTreeMap-ordered (alphabetical) — keys come out sorted.
    assert_eq!(
        prettify_args(r#"{"file_path":"/x","old_string":"a","new_string":"b"}"#),
        "file_path: /x, new_string: b, old_string: a"
    );
}

#[test]
fn prettify_args_nested_object_stays_as_json() {
    // Nested structure isn't unwrapped — keeps fidelity for tools like TodoWrite.
    let out = prettify_args(r#"{"todos":[{"content":"X","status":"pending"}]}"#);
    assert_eq!(out, r#"todos: [{"content":"X","status":"pending"}]"#);
}

#[test]
fn prettify_args_numeric_and_bool_values_kept_as_json() {
    let out = prettify_args(r#"{"timeout":5000,"force":true}"#);
    // Order is alphabetical (force, timeout).
    assert_eq!(out, "force: true, timeout: 5000");
}

#[test]
fn prettify_args_falls_back_to_raw_on_malformed_json() {
    assert_eq!(prettify_args("not json at all"), "not json at all");
}

#[test]
fn prettify_args_falls_back_to_raw_on_empty_object() {
    // An empty object would render as the empty string, which is worse than
    // showing `{}` — keep the raw form so the user sees something concrete.
    assert_eq!(prettify_args("{}"), "{}");
}

#[test]
fn prettify_args_falls_back_to_raw_on_non_object_root() {
    // Tool input is expected to be an object; arrays / strings / etc. shouldn't
    // happen in practice, but we don't try to munge them.
    assert_eq!(prettify_args(r#"[1, 2, 3]"#), "[1, 2, 3]");
    assert_eq!(prettify_args(r#""just a string""#), r#""just a string""#);
}

#[test]
fn prettify_args_shows_all_top_level_fields_no_field_hidden() {
    // Tool-agnostic display means no hostile input can hide a dangerous field
    // behind an alphabetically-earlier sibling — both come out in the preview.
    let out = prettify_args(r#"{"a_url":"https://safe.example","command":"rm -rf ~"}"#);
    assert!(out.contains("a_url: https://safe.example"));
    assert!(out.contains("command: rm -rf ~"));
}

#[test]
fn truncate_head_tail_under_limit_unchanged() {
    let s = "short string";
    assert_eq!(truncate_head_tail(s, 500, 80), s);
}

#[test]
fn truncate_head_tail_at_limit_unchanged() {
    let s = "x".repeat(500);
    assert_eq!(truncate_head_tail(&s, 500, 80), s);
}

#[test]
fn truncate_head_tail_over_limit_keeps_head_and_tail() {
    // Distinguishable head + tail so we can assert both are preserved.
    let s: String = (0..1000).map(|i| if i < 500 { 'A' } else { 'Z' }).collect();
    let out = truncate_head_tail(&s, 500, 80);
    assert!(out.starts_with("AAA"), "head AAAs preserved: {out}");
    assert!(out.ends_with("ZZZ"), "tail ZZZs preserved: {out}");
    assert!(out.contains("… [truncated] …"), "indicator visible: {out}");
}

#[test]
fn truncate_head_tail_handles_multibyte_chars_by_char_not_byte() {
    // 600 emoji, each multi-byte. Truncation must operate on `chars()` to
    // avoid splitting a code point.
    let s: String = "🦀".repeat(600);
    let out = truncate_head_tail(&s, 500, 80);
    assert!(out.contains("… [truncated] …"));
    // Head + tail = 500 emoji + indicator chars, all complete code points.
    assert!(
        out.chars()
            .all(|c| c == '🦀' || " …[truncated]".contains(c))
    );
}
