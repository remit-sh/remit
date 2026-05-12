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
    };
    let v: serde_json::Value = serde_json::from_slice(&serde_json::to_vec(&req).unwrap()).unwrap();
    assert_eq!(v["protocol"], 1);
    assert_eq!(v["kind"], "choose");
    assert_eq!(v["scope"], "s");
    assert_eq!(v["reason"], "r");
    assert_eq!(v["working_dir"], "/w");
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
