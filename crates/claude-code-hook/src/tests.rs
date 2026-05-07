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
