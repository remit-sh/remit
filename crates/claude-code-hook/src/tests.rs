use super::*;

#[test]
fn no_env_means_no_parent() {
    assert_eq!(
        parent_from_env_value(None, &AgentId::try_new("self").unwrap()),
        None
    );
}

#[test]
fn env_differing_from_self_is_parent() {
    let me = AgentId::try_new("self").unwrap();
    assert_eq!(
        parent_from_env_value(Some("parent-id".into()), &me),
        Some(AgentId::try_new("parent-id").unwrap())
    );
}

#[test]
fn env_equal_to_self_is_ignored() {
    let me = AgentId::try_new("self").unwrap();
    assert_eq!(parent_from_env_value(Some("self".into()), &me), None);
}

#[test]
fn env_with_invalid_id_is_ignored() {
    let me = AgentId::try_new("self").unwrap();
    assert_eq!(
        parent_from_env_value(Some("not a valid id".into()), &me),
        None
    );
}

#[test]
fn agent_tool_bundles_prompt_as_scope() {
    let input = serde_json::json!({
        "prompt": "Read README and report char count.",
        "subagent_type": "general-purpose",
    });
    assert_eq!(
        bundled_scope_from("Agent", &input),
        Some("Read README and report char count.".to_string())
    );
}

#[test]
fn unknown_tool_has_no_bundled_scope() {
    let input = serde_json::json!({ "prompt": "irrelevant" });
    assert_eq!(bundled_scope_from("Bash", &input), None);
}

#[test]
fn missing_field_has_no_bundled_scope() {
    let input = serde_json::json!({ "subagent_type": "general-purpose" });
    assert_eq!(bundled_scope_from("Agent", &input), None);
}

#[test]
fn empty_field_has_no_bundled_scope() {
    let input = serde_json::json!({ "prompt": "" });
    assert_eq!(bundled_scope_from("Agent", &input), None);
}

#[test]
fn whitespace_only_field_has_no_bundled_scope() {
    let input = serde_json::json!({ "prompt": "   \n\t " });
    assert_eq!(bundled_scope_from("Agent", &input), None);
}

#[test]
fn non_string_field_has_no_bundled_scope() {
    let input = serde_json::json!({ "prompt": 42 });
    assert_eq!(bundled_scope_from("Agent", &input), None);
}

#[test]
fn send_message_bundles_content_as_scope() {
    let input = serde_json::json!({
        "content": "Run the test suite and report failures.",
        "recipient": "scout",
        "to": "scout",
    });
    assert_eq!(
        bundled_scope_from("SendMessage", &input),
        Some("Run the test suite and report failures.".to_string())
    );
}

#[test]
fn send_message_missing_content_has_no_bundled_scope() {
    let input = serde_json::json!({ "recipient": "scout", "to": "scout" });
    assert_eq!(bundled_scope_from("SendMessage", &input), None);
}

#[test]
fn send_message_empty_content_has_no_bundled_scope() {
    let input = serde_json::json!({ "content": "", "recipient": "scout" });
    assert_eq!(bundled_scope_from("SendMessage", &input), None);
}

#[test]
fn send_message_whitespace_content_has_no_bundled_scope() {
    let input = serde_json::json!({ "content": "  \n\t ", "recipient": "scout" });
    assert_eq!(bundled_scope_from("SendMessage", &input), None);
}

#[test]
fn send_message_non_string_content_has_no_bundled_scope() {
    let input = serde_json::json!({ "content": { "type": "shutdown_request" } });
    assert_eq!(bundled_scope_from("SendMessage", &input), None);
}

/// Snapshot the bundled-scope allowlist so adding a new entry is a deliberate,
/// review-visible change. If you want to add a tool here, update the assertion too.
#[test]
fn bundled_scope_fields_allowlist_is_pinned() {
    assert_eq!(
        BUNDLED_SCOPE_FIELDS,
        &[("Agent", "prompt"), ("SendMessage", "content")]
    );
}

#[test]
fn strip_drops_top_level_narration_fields() {
    let mut v = serde_json::json!({
        "command": "ls",
        "description": "list root",
        "reason": "because",
        "note": "n",
        "comment": "c",
        "intent": "i",
        "purpose": "p",
        "rationale": "r",
        "justification": "j",
        "summary": "s",
    });
    strip_narration(&mut v);
    assert_eq!(v, serde_json::json!({ "command": "ls" }));
}

#[test]
fn strip_is_case_insensitive() {
    let mut v = serde_json::json!({
        "Description": "x",
        "REASON": "y",
        "Comment": "z",
        "ok": 1,
    });
    strip_narration(&mut v);
    assert_eq!(v, serde_json::json!({ "ok": 1 }));
}

#[test]
fn strip_descends_into_nested_objects() {
    let mut v = serde_json::json!({
        "outer": { "reason": "x", "keep": 1, "deep": { "note": "y", "stay": 2 } }
    });
    strip_narration(&mut v);
    assert_eq!(
        v,
        serde_json::json!({ "outer": { "keep": 1, "deep": { "stay": 2 } } })
    );
}

#[test]
fn strip_descends_into_arrays_of_objects() {
    let mut v = serde_json::json!({
        "items": [
            { "keep": 1, "note": "x" },
            { "keep": 2, "comment": "y" },
        ]
    });
    strip_narration(&mut v);
    assert_eq!(
        v,
        serde_json::json!({ "items": [{ "keep": 1 }, { "keep": 2 }] })
    );
}

#[test]
fn strip_preserves_non_object_root() {
    let mut s = serde_json::json!("hello");
    strip_narration(&mut s);
    assert_eq!(s, serde_json::json!("hello"));

    let mut n = serde_json::json!(42);
    strip_narration(&mut n);
    assert_eq!(n, serde_json::json!(42));

    let mut a = serde_json::json!([1, 2, 3]);
    strip_narration(&mut a);
    assert_eq!(a, serde_json::json!([1, 2, 3]));

    let mut null = serde_json::Value::Null;
    strip_narration(&mut null);
    assert_eq!(null, serde_json::Value::Null);
}

#[test]
fn strip_preserves_prompt_and_content() {
    let mut v = serde_json::json!({
        "prompt": "do thing",
        "content": "say hi",
        "nested": { "prompt": "deep", "content": "deeper" },
    });
    let original = v.clone();
    strip_narration(&mut v);
    assert_eq!(v, original);
}

#[test]
fn strip_no_op_when_no_narration() {
    let mut v = serde_json::json!({ "command": "ls", "args": ["-la"] });
    let original = v.clone();
    strip_narration(&mut v);
    assert_eq!(v, original);
}

#[test]
fn strip_preserves_null_value_for_kept_key() {
    let mut v = serde_json::json!({ "command": null, "args": [] });
    strip_narration(&mut v);
    assert_eq!(v, serde_json::json!({ "command": null, "args": [] }));
}

#[test]
fn strip_descends_into_array_of_array_of_objects() {
    let mut v = serde_json::json!([[{ "reason": "drop", "keep": 1 }]]);
    strip_narration(&mut v);
    assert_eq!(v, serde_json::json!([[{ "keep": 1 }]]));
}

#[test]
fn strip_drops_narration_key_whose_value_is_an_object() {
    let mut v = serde_json::json!({
        "reason": { "nested": "val", "deeper": { "more": 1 } },
        "keep": 1,
    });
    strip_narration(&mut v);
    assert_eq!(v, serde_json::json!({ "keep": 1 }));
}

#[test]
fn bundled_scope_extraction_survives_strip_on_same_payload() {
    // Guards the source-order invariant in handle_pre_tool_use:
    // bundled-scope extraction reads `prompt`/`content` from raw input;
    // strip then drops narration. Both paths must be intact on the same payload.
    let raw = serde_json::json!({
        "prompt": "do thing",
        "description": "narrator says hi",
    });
    assert_eq!(
        bundled_scope_from("Agent", &raw),
        Some("do thing".to_string())
    );
    let mut stripped = raw.clone();
    strip_narration(&mut stripped);
    assert_eq!(stripped, serde_json::json!({ "prompt": "do thing" }));
}

// ----- Agent type parsing (TN-2026-009 / #32) -----

#[test]
fn parse_agent_type_missing_yields_none() {
    assert_eq!(parse_agent_type(None), None);
}

#[test]
fn parse_agent_type_valid_yields_some() {
    assert_eq!(
        parse_agent_type(Some("architect")),
        Some(AgentType::try_new("architect").unwrap())
    );
    assert_eq!(
        parse_agent_type(Some("general-purpose")),
        Some(AgentType::try_new("general-purpose").unwrap())
    );
}

#[test]
fn parse_agent_type_invalid_yields_none_never_panics() {
    // Validation failure (path traversal, charset, length) must collapse to
    // None — never Red, per TN-2026-009 fail-closed table.
    assert_eq!(parse_agent_type(Some("")), None);
    assert_eq!(parse_agent_type(Some("../etc/passwd")), None);
    assert_eq!(parse_agent_type(Some("foo bar")), None);
    assert_eq!(parse_agent_type(Some("Ärchitect")), None);
}

#[test]
fn pretooluse_payload_deserialises_agent_type_when_present() {
    let json = serde_json::json!({
        "session_id": "sess",
        "agent_id": "child",
        "agent_type": "architect",
        "tool_name": "Bash",
        "tool_input": {},
        "cwd": "/tmp",
    });
    let payload: PreToolUsePayload = serde_json::from_value(json).unwrap();
    assert_eq!(payload.agent_type.as_deref(), Some("architect"));
}

#[test]
fn pretooluse_payload_agent_type_absent_is_none() {
    let json = serde_json::json!({
        "session_id": "sess",
        "tool_name": "Bash",
        "tool_input": {},
        "cwd": "/tmp",
    });
    let payload: PreToolUsePayload = serde_json::from_value(json).unwrap();
    assert!(payload.agent_type.is_none());
}

// ----- format_red_for_agent: stderr rendering on Red -----

use domain::{AgentStore, Scope, ScopeStoreError};
use std::cell::RefCell;
use std::collections::HashMap;

/// In-memory `AgentStore` mock for testing `format_red_for_agent` without
/// touching the filesystem. Only the methods the function uses are non-trivial;
/// the rest are stubs that panic so an unintended call surfaces in tests.
///
/// A richer fake (`FakeAgentStore`) exists in `crates/domain/tests/common/mod.rs`
/// but it's tests-only inside the domain crate and not exported. This minimal
/// duplicate covers the one read path this composition-root helper exercises.
struct MockStore {
    scopes: RefCell<HashMap<AgentId, Scope>>,
    fail_read: bool,
}

impl MockStore {
    fn empty() -> Self {
        Self {
            scopes: RefCell::new(HashMap::new()),
            fail_read: false,
        }
    }

    fn with_scope(id: &AgentId, scope: Scope) -> Self {
        let me = Self::empty();
        me.scopes.borrow_mut().insert(id.clone(), scope);
        me
    }

    fn failing() -> Self {
        Self {
            scopes: RefCell::new(HashMap::new()),
            fail_read: true,
        }
    }
}

impl AgentStore for MockStore {
    fn scope_for(&self, id: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        if self.fail_read {
            return Err(ScopeStoreError::Unavailable("mock failure".into()));
        }
        Ok(self.scopes.borrow().get(id).cloned())
    }
    fn assign_scope(&self, _: &AgentId, _: Scope) -> Result<(), ScopeStoreError> {
        unimplemented!("not used by format_red_for_agent")
    }
    fn set_pending(&self, _: &AgentId, _: Scope) -> Result<(), ScopeStoreError> {
        unimplemented!()
    }
    fn take_pending(&self, _: &AgentId) -> Result<Option<Scope>, ScopeStoreError> {
        unimplemented!()
    }
    fn is_disabled(&self, _: &AgentId) -> Result<bool, ScopeStoreError> {
        unimplemented!()
    }
    fn disable(&self, _: &AgentId) -> Result<(), ScopeStoreError> {
        unimplemented!()
    }
}

#[test]
fn format_red_for_agent_appends_scope_when_present() {
    let id = AgentId::try_new("agent-1").unwrap();
    let scope = Scope::try_new("develop the frontend".to_string()).unwrap();
    let store = MockStore::with_scope(&id, scope);
    let out = format_red_for_agent("Action reads ~/.ssh", &store, &id);
    assert_eq!(
        out,
        "Action reads ~/.ssh\n\nYour scope: develop the frontend"
    );
}

#[test]
fn format_red_for_agent_omits_scope_when_absent() {
    let id = AgentId::try_new("agent-1").unwrap();
    let store = MockStore::empty();
    let out = format_red_for_agent("Action reads ~/.ssh", &store, &id);
    assert_eq!(out, "Action reads ~/.ssh");
}

#[test]
fn format_red_for_agent_omits_scope_when_store_errors() {
    let id = AgentId::try_new("agent-1").unwrap();
    let store = MockStore::failing();
    // Fail-soft on the stderr-formatting path: Red has already been decided;
    // we still emit the reason. The store error is not surfaced here.
    let out = format_red_for_agent("Action reads ~/.ssh", &store, &id);
    assert_eq!(out, "Action reads ~/.ssh");
}

/// Snapshot the narration denylist so adding a new entry is a deliberate,
/// review-visible change. If you want to add a field here, update the assertion too.
#[test]
fn narration_fields_denylist_is_pinned() {
    assert_eq!(
        NARRATION_FIELDS,
        &[
            "description",
            "reason",
            "note",
            "comment",
            "intent",
            "purpose",
            "rationale",
            "justification",
            "summary",
        ]
    );
}
