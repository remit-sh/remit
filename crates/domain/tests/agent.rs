use domain::{Agent, AgentId};

fn agent_id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

#[test]
fn root_has_no_parent() {
    assert!(!Agent::root(agent_id("solo")).has_parent());
}

#[test]
fn subagent_has_parent_when_distinct_from_self() {
    assert!(Agent::subagent(agent_id("child"), agent_id("parent")).has_parent());
}

#[test]
fn subagent_collapses_self_referential_parent_to_no_parent() {
    let id = agent_id("loop");
    let agent = Agent::subagent(id.clone(), id);
    assert!(!agent.has_parent());
    assert!(agent.parent_id.is_none());
}
