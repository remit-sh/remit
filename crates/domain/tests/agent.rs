use domain::{Agent, AgentId};

fn agent_id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

#[test]
fn root_has_no_parent() {
    let agent = Agent::root(agent_id("solo"));
    assert!(matches!(agent, Agent::Root { .. }));
    assert!(agent.parent_id().is_none());
}

#[test]
fn subagent_has_parent_when_distinct_from_self() {
    let agent = Agent::subagent(agent_id("child"), agent_id("parent"));
    assert!(matches!(agent, Agent::Subagent { .. }));
    assert_eq!(agent.parent_id().map(|p| p.as_ref()), Some("parent"));
}

#[test]
fn subagent_collapses_self_referential_parent_to_root() {
    let id = agent_id("loop");
    let agent = Agent::subagent(id.clone(), id);
    assert!(matches!(agent, Agent::Root { .. }));
    assert!(agent.parent_id().is_none());
}
