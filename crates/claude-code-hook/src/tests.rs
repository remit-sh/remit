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
        bundled_text_from("Agent", &input),
        Some("Read README and report char count.")
    );
}

#[test]
fn unknown_tool_has_no_bundled_text() {
    let input = serde_json::json!({ "prompt": "irrelevant" });
    assert_eq!(bundled_text_from("Bash", &input), None);
}

#[test]
fn missing_field_is_empty_bundled_text() {
    let input = serde_json::json!({ "subagent_type": "general-purpose" });
    assert_eq!(bundled_text_from("Agent", &input), Some(""));
}

#[test]
fn empty_field_is_empty_bundled_text() {
    let input = serde_json::json!({ "prompt": "" });
    assert_eq!(bundled_text_from("Agent", &input), Some(""));
}

#[test]
fn whitespace_only_field_is_kept_as_bundled_text() {
    let input = serde_json::json!({ "prompt": "   \n\t " });
    assert_eq!(
        bundled_text_from("Agent", &input),
        Some(
            "   
	 "
        )
    );
}

#[test]
fn non_string_field_is_empty_bundled_text() {
    let input = serde_json::json!({ "prompt": 42 });
    assert_eq!(bundled_text_from("Agent", &input), Some(""));
}

#[test]
fn send_message_bundles_content_as_scope() {
    let input = serde_json::json!({
        "content": "Run the test suite and report failures.",
        "recipient": "scout",
        "to": "scout",
    });
    assert_eq!(
        bundled_text_from("SendMessage", &input),
        Some("Run the test suite and report failures.")
    );
}

#[test]
fn send_message_missing_content_is_empty_bundled_text() {
    let input = serde_json::json!({ "recipient": "scout", "to": "scout" });
    assert_eq!(bundled_text_from("SendMessage", &input), Some(""));
}

#[test]
fn send_message_empty_content_is_empty_bundled_text() {
    let input = serde_json::json!({ "content": "", "recipient": "scout" });
    assert_eq!(bundled_text_from("SendMessage", &input), Some(""));
}

#[test]
fn send_message_whitespace_content_is_kept_as_bundled_text() {
    let input = serde_json::json!({ "content": "  \n\t ", "recipient": "scout" });
    assert_eq!(
        bundled_text_from("SendMessage", &input),
        Some(
            "  
	 "
        )
    );
}

#[test]
fn send_message_non_string_content_is_empty_bundled_text() {
    let input = serde_json::json!({ "content": { "type": "shutdown_request" } });
    assert_eq!(bundled_text_from("SendMessage", &input), Some(""));
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
    assert_eq!(bundled_text_from("Agent", &raw), Some("do thing"));
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
    fn clear_pending(&self, _: &AgentId) -> Result<(), ScopeStoreError> {
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

// ----- dev_bypass_hint: only surfaces for ScopeSetupUnavailable -----

#[test]
fn dev_bypass_hint_present_for_scope_setup_unavailable() {
    let hint = dev_bypass_hint(&RedKind::ScopeSetupUnavailable)
        .expect("ScopeSetupUnavailable gets a hint");
    assert!(hint.contains("dev_bypass"));
    assert!(hint.contains("~/.remit/config.toml"));
}

#[test]
fn dev_bypass_hint_absent_for_other_red_kinds() {
    // Genuine Reds (out-of-scope, store failure, classifier failure) must not
    // suggest dev-bypass — that would invite a user to silence a correct deny.
    for kind in [
        RedKind::NoScopeRoot,
        RedKind::ScopeSetupCancelled,
        RedKind::SubagentMissingScope,
        RedKind::StoreUnavailable,
        RedKind::OutOfScope,
        RedKind::ClassifierUnavailable,
        RedKind::StandingGuidanceUnavailable,
    ] {
        assert!(
            dev_bypass_hint(&kind).is_none(),
            "expected no hint for {kind:?}"
        );
    }
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

mod user_prompt_submit {
    use domain::{Agent, AgentId, AssignExtractedScopeResult, LlmClassifierError, ScopeStoreError};

    use crate::agent_from;
    use crate::user_prompt::{UserPromptSubmitPayload, outcome_parts};

    fn id(s: &str) -> AgentId {
        AgentId::try_new(s).unwrap()
    }

    #[test]
    fn payload_parses_the_traced_wire_shape() {
        // Shape per docs/diag/user-prompt-submit-dump.log.
        let raw = serde_json::json!({
            "session_id": "4a7b3990-fa86-42ae-8393-068a4dc04fd8",
            "transcript_path": "/x.jsonl",
            "cwd": "/proj",
            "prompt_id": "29fd0ee8",
            "permission_mode": "default",
            "hook_event_name": "UserPromptSubmit",
            "prompt": "/diag-cmd 42",
        });
        assert!(serde_json::from_value::<UserPromptSubmitPayload>(raw).is_ok());
    }

    #[test]
    fn payload_without_prompt_is_rejected() {
        let raw = serde_json::json!({ "session_id": "s" });
        assert!(serde_json::from_value::<UserPromptSubmitPayload>(raw).is_err());
    }

    #[test]
    fn session_without_parent_is_a_root_agent() {
        assert_eq!(
            agent_from(id("sess"), None, |_| None),
            Agent::root(id("sess"))
        );
    }

    #[test]
    fn session_with_env_parent_is_a_subagent() {
        assert_eq!(
            agent_from(id("sess"), None, |_| Some(id("parent"))),
            Agent::subagent(id("sess"), id("parent"))
        );
    }

    #[test]
    fn payload_agent_id_is_a_subagent_of_the_session() {
        assert_eq!(
            agent_from(id("sess"), Some(id("child")), |_| None),
            Agent::subagent(id("child"), id("sess"))
        );
    }

    #[test]
    fn outcome_labels_are_pinned() {
        let llm_err = || LlmClassifierError::Unavailable("llm down".into());
        let store_err = || ScopeStoreError::Unavailable("disk".into());
        assert_eq!(
            outcome_parts(&AssignExtractedScopeResult::Assigned),
            ("assigned", None)
        );
        assert_eq!(
            outcome_parts(&AssignExtractedScopeResult::NotRoot),
            ("not_root", None)
        );
        assert_eq!(
            outcome_parts(&AssignExtractedScopeResult::Disabled),
            ("disabled", None)
        );
        assert_eq!(
            outcome_parts(&AssignExtractedScopeResult::AlreadyScoped),
            ("already_scoped", None)
        );
        assert_eq!(
            outcome_parts(&AssignExtractedScopeResult::NoScope),
            ("no_scope", None)
        );
        assert_eq!(
            outcome_parts(&AssignExtractedScopeResult::ExtractionUnavailable(llm_err())),
            ("extraction_unavailable", Some("llm down".into()))
        );
        assert_eq!(
            outcome_parts(&AssignExtractedScopeResult::StoreUnavailable(store_err())),
            (
                "store_unavailable",
                Some("scope backend unavailable".into())
            )
        );
    }
}

mod stage_pending_outcome {
    use domain::{
        AgentId, AgentStore, LlmClassifierError, Scope, ScopeStoreError, StageExtractedScopeResult,
    };
    use file_classification_history::FileClassificationHistoryWriter;
    use file_scope_store::FileAgentStore;

    use crate::bundled_scope::{discard_pending, stage_outcome_parts};

    #[test]
    fn outcome_labels_are_pinned() {
        assert_eq!(
            stage_outcome_parts(&StageExtractedScopeResult::Staged),
            ("staged", None)
        );
        assert_eq!(
            stage_outcome_parts(&StageExtractedScopeResult::NoScope),
            ("no_scope", None)
        );
        assert_eq!(
            stage_outcome_parts(&StageExtractedScopeResult::ExtractionUnavailable(
                LlmClassifierError::Unavailable("llm down".into())
            )),
            ("extraction_unavailable", Some("llm down".into()))
        );
        assert_eq!(
            stage_outcome_parts(&StageExtractedScopeResult::StoreUnavailable(
                ScopeStoreError::Unavailable("disk".into())
            )),
            (
                "store_unavailable",
                Some("scope backend unavailable".into())
            )
        );
    }

    #[test]
    fn discard_pending_drops_the_row_staged_for_a_denied_spawn() {
        let dir = tempfile::tempdir().unwrap();
        let agent_store = FileAgentStore::new(dir.path().join("agents"));
        let log_path = dir.path().join("remit.log");
        let writer = FileClassificationHistoryWriter::new(log_path.clone());
        let parent = AgentId::try_new("parent").unwrap();
        agent_store
            .set_pending(
                &parent,
                Scope::try_new("staged for a denied spawn").unwrap(),
            )
            .unwrap();

        discard_pending(&parent, &agent_store, &writer);

        assert_eq!(agent_store.take_pending(&parent).unwrap(), None);
        let log = std::fs::read_to_string(log_path).unwrap();
        assert!(log.contains("stage_pending: discarded"));
        assert!(!log.contains("staged for a denied spawn"));
    }
}
