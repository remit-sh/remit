use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, Agent, AgentId, ClassificationInput, Classifier, RedKind,
    Scope, Verdict, process_action,
};

mod common;
use common::{FakePrompter, FakeReader, LoggedVerdict, RecordingLogger, RecordingWriter};

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
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let v = process_action(
        &input(Agent::root(id("sess"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
    );
    assert!(matches!(v, Verdict::Green));
}

#[test]
fn green_after_successful_prompt_and_persist() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::returning(scope("freshly typed"));
    let writer = RecordingWriter::new();
    let writes = writer.writes();
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let v = process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
    );
    assert!(matches!(v, Verdict::Green));
    assert_eq!(writes.borrow().len(), 1);
}

#[test]
fn red_scope_setup_cancelled_when_prompt_cancelled() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::cancelled();
    let writer = RecordingWriter::new();
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let Verdict::Red { kind, .. } = process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
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
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let Verdict::Red { kind, .. } = process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
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
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let Verdict::Red { kind, .. } = process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
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
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let Verdict::Red { kind, .. } = process_action(
        &input(Agent::subagent(id("child"), id("parent"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::SubagentMissingScope);
    assert_eq!(*prompt_calls.borrow(), 0);
}

#[test]
fn logs_incoming_and_outcome_on_green() {
    let reader = FakeReader::new().with(id("sess"), scope("do the thing"));
    let prompter = FakePrompter::returning(scope("nope"));
    let writer = RecordingWriter::new();
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let incoming = logger.call_count();
    let outcomes = logger.outcomes();
    let errors = logger.errors();
    process_action(
        &input(Agent::root(id("sess"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
    );
    assert_eq!(*incoming.borrow(), 1);
    assert_eq!(outcomes.borrow().as_slice(), &[LoggedVerdict::Green]);
    assert!(errors.borrow().is_empty());
}

#[test]
fn logs_final_verdict_after_no_scope_root_transformation() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::cancelled();
    let writer = RecordingWriter::new();
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let outcomes = logger.outcomes();
    process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
    );
    assert_eq!(
        outcomes.borrow().as_slice(),
        &[LoggedVerdict::Red(RedKind::ScopeSetupCancelled)],
        "outcome log must record final ScopeSetupCancelled, not the intermediate NoScopeRoot"
    );
}

#[test]
fn logs_error_and_outcome_on_store_failure() {
    let reader = FakeReader::failing();
    let prompter = FakePrompter::returning(scope("nope"));
    let writer = RecordingWriter::new();
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let incoming = logger.call_count();
    let outcomes = logger.outcomes();
    let errors = logger.errors();
    process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
    );
    assert_eq!(
        *incoming.borrow(),
        1,
        "log_incoming fires on error path too"
    );
    assert_eq!(
        outcomes.borrow().as_slice(),
        &[LoggedVerdict::Red(RedKind::StoreUnavailable)]
    );
    assert_eq!(errors.borrow().len(), 1);
    let logged = &errors.borrow()[0];
    assert!(
        logged.contains("scope store") && logged.contains("fake failure"),
        "expected source chain to reach inner cause, got: {logged}"
    );
}

#[test]
fn logs_error_and_outcome_on_prompt_failure() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::unavailable();
    let writer = RecordingWriter::new();
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let outcomes = logger.outcomes();
    let errors = logger.errors();
    process_action(
        &input(Agent::root(id("root"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
    );
    assert_eq!(
        outcomes.borrow().as_slice(),
        &[LoggedVerdict::Red(RedKind::ScopeSetupUnavailable)]
    );
    assert_eq!(errors.borrow().len(), 1);
    let logged = &errors.borrow()[0];
    assert!(
        logged.contains("prompt") && logged.contains("fake failure"),
        "expected source chain to reach inner cause, got: {logged}"
    );
}

#[test]
fn does_not_log_error_on_green() {
    let reader = FakeReader::new().with(id("sess"), scope("do the thing"));
    let prompter = FakePrompter::returning(scope("nope"));
    let writer = RecordingWriter::new();
    let classifier = Classifier::new();
    let logger = RecordingLogger::new();
    let errors = logger.errors();
    process_action(
        &input(Agent::root(id("sess"))),
        &reader,
        &prompter,
        &writer,
        &classifier,
        &logger,
    );
    assert!(errors.borrow().is_empty());
}
