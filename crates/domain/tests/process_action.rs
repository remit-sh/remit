use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationInput, Classifier, RedKind,
    Scope, Verdict, process_action,
};

mod common;
use common::{FakePrompter, FakeReader, RecordingLogger, RecordingWriter};

fn id(s: &str) -> AgentId {
    AgentId::try_new(s).unwrap()
}

fn scope(s: &str) -> Scope {
    Scope::try_new(s).unwrap()
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
fn green_when_existing_scope_present() {
    let reader = FakeReader::new().with(id("sess"), scope("do the thing"));
    let prompter = FakePrompter::returning(scope("nope"));
    let writer = RecordingWriter::new();
    let classifier = Classifier::new(RecordingLogger::new());
    let v = process_action(
        &input(Agent::root(id("sess"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
    );
    assert!(matches!(v, Verdict::Green));
}

#[test]
fn green_after_successful_prompt_and_persist() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::returning(scope("freshly typed"));
    let writer = RecordingWriter::new();
    let writes = writer.writes();
    let classifier = Classifier::new(RecordingLogger::new());
    let v = process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
    );
    assert!(matches!(v, Verdict::Green));
    assert_eq!(writes.borrow().len(), 1);
}

#[test]
fn red_scope_setup_cancelled_when_prompt_cancelled() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::cancelled();
    let writer = RecordingWriter::new();
    let classifier = Classifier::new(RecordingLogger::new());
    let Verdict::Red { kind, .. } = process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::ScopeSetupCancelled);
}

#[test]
fn red_scope_setup_unavailable_when_prompt_fails() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::unavailable();
    let writer = RecordingWriter::new();
    let classifier = Classifier::new(RecordingLogger::new());
    let Verdict::Red { kind, .. } = process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::ScopeSetupUnavailable);
}

#[test]
fn red_store_unavailable_when_reader_fails() {
    let reader = FakeReader::failing();
    let prompter = FakePrompter::returning(scope("nope"));
    let writer = RecordingWriter::new();
    let classifier = Classifier::new(RecordingLogger::new());
    let Verdict::Red { kind, .. } = process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::StoreUnavailable);
}

#[test]
fn red_subagent_missing_scope_when_subagent_has_no_bound_scope() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::returning(scope("never used"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let classifier = Classifier::new(RecordingLogger::new());
    let Verdict::Red { kind, .. } = process_action(
        &input(Agent::subagent(id("child"), id("parent"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::SubagentMissingScope);
    assert_eq!(*prompt_calls.borrow(), 0);
}
