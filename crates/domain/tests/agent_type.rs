//! Tests for [`domain::AgentType`]. Same charset and length rules as [`AgentId`];
//! rejects path-traversal characters so `<type>.md` lookup inside
//! `.remit/agent-templates/` is safe by construction.

use domain::AgentType;

#[test]
fn accepts_simple_ascii_alphanumeric() {
    AgentType::try_new("architect").unwrap();
    AgentType::try_new("general-purpose").unwrap();
    AgentType::try_new("test_writer").unwrap();
    AgentType::try_new("agent42").unwrap();
}

#[test]
fn rejects_empty() {
    assert!(AgentType::try_new("").is_err());
}

#[test]
fn rejects_oversized() {
    let s = "a".repeat(129);
    assert!(AgentType::try_new(s).is_err());
}

#[test]
fn accepts_exactly_max_length() {
    let s = "a".repeat(128);
    AgentType::try_new(s).unwrap();
}

#[test]
fn rejects_path_separator() {
    assert!(AgentType::try_new("../etc/passwd").is_err());
    assert!(AgentType::try_new("foo/bar").is_err());
}

#[test]
fn rejects_dot_traversal() {
    assert!(AgentType::try_new("..").is_err());
    assert!(AgentType::try_new(".hidden").is_err());
}

#[test]
fn rejects_whitespace() {
    assert!(AgentType::try_new("foo bar").is_err());
    assert!(AgentType::try_new("\tarchitect").is_err());
}

#[test]
fn rejects_non_ascii() {
    assert!(AgentType::try_new("Ärchitect").is_err());
    assert!(AgentType::try_new("test\u{200B}writer").is_err());
}
