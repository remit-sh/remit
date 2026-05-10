use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationInput, Classifier, RedKind,
    Scope, StandingGuidance, Verdict,
};

mod common;
use common::FakeLlmClassifier;

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
fn green_when_scope_present_and_llm_says_green() {
    let llm = FakeLlmClassifier::green();
    let llm_calls = llm.call_count();
    let classifier = Classifier::new(llm);
    let scope = Scope::try_new("do the thing").unwrap();
    assert!(matches!(
        classifier.classify(
            &input(Agent::root(id("sess"))),
            Some(&scope),
            &StandingGuidance::default()
        ),
        Verdict::Green
    ));
    assert_eq!(*llm_calls.borrow(), 1);
}

#[test]
fn red_out_of_scope_when_llm_says_red() {
    let llm = FakeLlmClassifier::red("reads private SSH key, unrelated to scope");
    let classifier = Classifier::new(llm);
    let scope = Scope::try_new("do the thing").unwrap();
    let Verdict::Red { kind, reason } = classifier.classify(
        &input(Agent::root(id("sess"))),
        Some(&scope),
        &StandingGuidance::default(),
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::OutOfScope);
    assert!(reason.contains("SSH key"));
}

#[test]
fn red_classifier_unavailable_when_llm_fails() {
    let llm = FakeLlmClassifier::unavailable();
    let classifier = Classifier::new(llm);
    let scope = Scope::try_new("do the thing").unwrap();
    let Verdict::Red { kind, reason } = classifier.classify(
        &input(Agent::root(id("sess"))),
        Some(&scope),
        &StandingGuidance::default(),
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::ClassifierUnavailable);
    assert!(reason.contains("classifier unavailable"));
}

#[test]
fn red_no_scope_root_when_scope_absent_and_no_parent() {
    let llm = FakeLlmClassifier::green();
    let llm_calls = llm.call_count();
    let classifier = Classifier::new(llm);
    let Verdict::Red { kind, reason } = classifier.classify(
        &input(Agent::root(id("sess"))),
        None,
        &StandingGuidance::default(),
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::NoScopeRoot);
    assert!(reason.contains("no scope"));
    assert_eq!(*llm_calls.borrow(), 0);
}

#[test]
fn red_subagent_missing_scope_when_scope_absent_and_subagent() {
    let llm = FakeLlmClassifier::green();
    let llm_calls = llm.call_count();
    let classifier = Classifier::new(llm);
    let Verdict::Red { kind, reason } = classifier.classify(
        &input(Agent::subagent(id("child"), id("parent"))),
        None,
        &StandingGuidance::default(),
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::SubagentMissingScope);
    assert!(reason.contains("subagent has no scope"));
    assert_eq!(*llm_calls.borrow(), 0);
}
