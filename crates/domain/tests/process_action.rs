use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, ClassificationInput, Classifier, RedKind, Scope, Session,
    SessionId, Verdict, process_action,
};

mod common;
use common::{FakePrompter, FakeReader, RecordingLogger, RecordingWriter};

fn input(session_id: &str, parent: Option<&str>) -> ClassificationInput {
    ClassificationInput {
        session: Session {
            id: SessionId::try_new(session_id).unwrap(),
            parent_id: parent.map(|p| SessionId::try_new(p).unwrap()),
        },
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
    let id = SessionId::try_new("sess").unwrap();
    let reader = FakeReader::new().with(id, Scope::new("do the thing"));
    let prompter = FakePrompter::returning(Scope::new("nope"));
    let writer = RecordingWriter::new();
    let classifier = Classifier::new(RecordingLogger::new());
    let v = process_action(
        &input("sess", None),
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
    let prompter = FakePrompter::returning(Scope::new("freshly typed"));
    let writer = RecordingWriter::new();
    let writes = writer.writes();
    let classifier = Classifier::new(RecordingLogger::new());
    let v = process_action(
        &input("root", None),
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
        &input("root", None),
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
        &input("root", None),
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
    let prompter = FakePrompter::returning(Scope::new("nope"));
    let writer = RecordingWriter::new();
    let classifier = Classifier::new(RecordingLogger::new());
    let Verdict::Red { kind, .. } = process_action(
        &input("root", None),
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
fn red_subagent_parent_missing_scope_when_subagent_inheritance_fails() {
    let reader = FakeReader::new();
    let prompter = FakePrompter::returning(Scope::new("never used"));
    let prompt_calls = prompter.call_count();
    let writer = RecordingWriter::new();
    let classifier = Classifier::new(RecordingLogger::new());
    let Verdict::Red { kind, .. } = process_action(
        &input("child", Some("parent")),
        &reader,
        &prompter,
        &writer,
        &classifier,
    ) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::SubagentParentMissingScope);
    assert_eq!(*prompt_calls.borrow(), 0);
}
