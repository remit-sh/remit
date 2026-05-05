use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationInput, Classifier, RedKind,
    Scope, Verdict,
};

fn id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

fn input(agent: Agent) -> ClassificationInput {
    ClassificationInput {
        agent,
        action: Action {
            call_id: None,
            kind: ActionKind::new("Bash"),
            args: ActionArgs::new("ls"),
        },
        working_dir: PathBuf::from("/tmp"),
    }
}

#[test]
fn green_when_scope_is_present() {
    let classifier = Classifier::new();
    let scope = Scope::try_new("do the thing").unwrap();
    assert!(matches!(
        classifier.classify(&input(Agent::root(id("sess"))), Some(&scope)),
        Verdict::Green
    ));
}

#[test]
fn red_no_scope_root_when_scope_absent_and_no_parent() {
    let classifier = Classifier::new();
    let Verdict::Red { kind, reason } = classifier.classify(&input(Agent::root(id("sess"))), None)
    else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::NoScopeRoot);
    assert!(reason.contains("no scope"));
}

#[test]
fn red_subagent_missing_scope_when_scope_absent_and_subagent() {
    let classifier = Classifier::new();
    let Verdict::Red { kind, reason } =
        classifier.classify(&input(Agent::subagent(id("child"), id("parent"))), None)
    else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::SubagentMissingScope);
    assert!(reason.contains("subagent has no scope"));
}
