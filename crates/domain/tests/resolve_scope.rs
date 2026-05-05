use domain::{Agent, AgentId, Scope, resolve_scope};

mod common;
use common::FakeReader;

fn id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

#[test]
fn returns_agent_scope_when_present() {
    let scope = Scope::try_new("child task").unwrap();
    let reader = FakeReader::new().with(id("child"), scope.clone());
    let got = resolve_scope(&reader, &Agent::root(id("child"))).unwrap();
    assert_eq!(got, Some(scope));
}

#[test]
fn returns_none_when_subagent_has_no_own_scope_even_if_parent_does() {
    let scope = Scope::try_new("parent task").unwrap();
    let reader = FakeReader::new().with(id("parent"), scope);
    let agent = Agent::subagent(id("child"), id("parent"));
    let got = resolve_scope(&reader, &agent).unwrap();
    assert_eq!(got, None, "no parent-walk inheritance under bundled-scope");
}

#[test]
fn returns_none_when_root_agent_has_no_scope() {
    let reader = FakeReader::new();
    let got = resolve_scope(&reader, &Agent::root(id("solo"))).unwrap();
    assert_eq!(got, None);
}

#[test]
fn propagates_error_from_reader() {
    let reader = FakeReader::failing();
    let result = resolve_scope(&reader, &Agent::root(id("solo")));
    assert!(result.is_err());
}
