use domain::{Agent, AgentId, Scope, resolve_scope};

mod common;
use common::FakeAgentStore;

fn id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

#[test]
fn returns_agent_scope_when_present() {
    let scope = Scope::try_new("child task").unwrap();
    let store = FakeAgentStore::new().with_scope(id("child"), scope.clone());
    let got = resolve_scope(&store, &Agent::root(id("child"))).unwrap();
    assert_eq!(got, Some(scope));
}

#[test]
fn returns_none_when_subagent_has_no_own_scope_even_if_parent_does() {
    let scope = Scope::try_new("parent task").unwrap();
    let store = FakeAgentStore::new().with_scope(id("parent"), scope);
    let agent = Agent::subagent(id("child"), id("parent"));
    let got = resolve_scope(&store, &agent).unwrap();
    assert_eq!(got, None, "no parent-walk inheritance under bundled-scope");
}

#[test]
fn returns_none_when_root_agent_has_no_scope() {
    let store = FakeAgentStore::new();
    let got = resolve_scope(&store, &Agent::root(id("solo"))).unwrap();
    assert_eq!(got, None);
}

#[test]
fn propagates_error_from_store() {
    let store = FakeAgentStore::new().fail_scope_reads();
    let result = resolve_scope(&store, &Agent::root(id("solo")));
    assert!(result.is_err());
}
