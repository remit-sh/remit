use std::path::PathBuf;

use domain::{
    Action, ActionArgs, ActionKind, ClassificationInput, Classifier, RedKind, Scope, Session,
    SessionId, Verdict,
};

mod common;
use common::RecordingLogger;

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
fn green_when_scope_is_present() {
    let logger = RecordingLogger::new();
    let calls = logger.call_count();
    let classifier = Classifier::new(logger);
    let scope = Scope::new("do the thing");
    assert!(matches!(
        classifier.classify(&input("sess", None), Some(&scope)),
        Verdict::Green
    ));
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn red_no_scope_root_when_scope_absent_and_no_parent() {
    let logger = RecordingLogger::new();
    let calls = logger.call_count();
    let classifier = Classifier::new(logger);
    let Verdict::Red { kind, reason } = classifier.classify(&input("sess", None), None) else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::NoScopeRoot);
    assert!(reason.contains("no scope"));
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn red_subagent_parent_missing_scope_when_scope_absent_and_subagent() {
    let logger = RecordingLogger::new();
    let classifier = Classifier::new(logger);
    let Verdict::Red { kind, reason } = classifier.classify(&input("child", Some("parent")), None)
    else {
        panic!("expected Red");
    };
    assert_eq!(kind, RedKind::SubagentParentMissingScope);
    assert!(reason.contains("parent"));
}
