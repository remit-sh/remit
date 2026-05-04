use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, ClassificationInput, Classifier, Scope, Session, SessionId,
    Verdict,
};

mod common;
use common::{FakeReader, RecordingLogger};

fn input(session_id: &str) -> ClassificationInput {
    ClassificationInput {
        session: Session {
            id: SessionId::try_new(session_id).unwrap(),
            subagent_id: None,
            parent_id: None,
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
fn green_when_session_has_scope() {
    let id = SessionId::try_new("sess").unwrap();
    let reader = FakeReader::new().with(id, Scope::new("do the thing"));
    let logger = RecordingLogger::new();
    let calls = logger.call_count();
    let classifier = Classifier::new(logger, reader);
    assert!(matches!(
        classifier.classify(&input("sess")),
        Verdict::Green
    ));
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn red_when_no_scope_set() {
    let reader = FakeReader::new();
    let logger = RecordingLogger::new();
    let calls = logger.call_count();
    let classifier = Classifier::new(logger, reader);
    let Verdict::Red { reason } = classifier.classify(&input("sess")) else {
        panic!("expected Red");
    };
    assert!(reason.contains("no scope"));
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn red_when_scope_store_fails() {
    let logger = RecordingLogger::new();
    let calls = logger.call_count();
    let classifier = Classifier::new(logger, FakeReader::failing());
    let Verdict::Red { reason } = classifier.classify(&input("sess")) else {
        panic!("expected Red");
    };
    assert!(reason.contains("unavailable"));
    assert_eq!(*calls.borrow(), 1);
}
