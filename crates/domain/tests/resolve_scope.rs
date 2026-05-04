use domain::{Scope, Session, SessionId, resolve_scope};

mod common;
use common::FakeReader;

fn session(id: &str, parent: Option<&str>) -> Session {
    Session {
        id: SessionId::try_new(id).unwrap(),
        subagent_id: None,
        parent_id: parent.map(|p| SessionId::try_new(p).unwrap()),
    }
}

#[test]
fn returns_session_scope_when_present() {
    let id = SessionId::try_new("child").unwrap();
    let scope = Scope::new("child task");
    let reader = FakeReader::new().with(id, scope.clone());
    let got = resolve_scope(&reader, &session("child", None)).unwrap();
    assert_eq!(got, Some(scope));
}

#[test]
fn falls_back_to_parent_scope_when_session_missing() {
    let parent_id = SessionId::try_new("parent").unwrap();
    let scope = Scope::new("parent task");
    let reader = FakeReader::new().with(parent_id, scope.clone());
    let got = resolve_scope(&reader, &session("child", Some("parent"))).unwrap();
    assert_eq!(got, Some(scope));
}

#[test]
fn returns_none_when_neither_session_nor_parent_has_scope() {
    let reader = FakeReader::new();
    let got = resolve_scope(&reader, &session("child", Some("parent"))).unwrap();
    assert_eq!(got, None);
}

#[test]
fn returns_none_when_no_parent_and_session_missing() {
    let reader = FakeReader::new();
    let got = resolve_scope(&reader, &session("child", None)).unwrap();
    assert_eq!(got, None);
}

#[test]
fn ignores_self_referential_parent() {
    let id = SessionId::try_new("self").unwrap();
    let reader = FakeReader::new();
    let s = Session {
        id: id.clone(),
        subagent_id: None,
        parent_id: Some(id),
    };
    assert_eq!(resolve_scope(&reader, &s).unwrap(), None);
}

#[test]
fn propagates_error_from_parent_lookup() {
    let parent_id = SessionId::try_new("parent").unwrap();
    let reader = FakeReader::new().failing_for(parent_id);
    let result = resolve_scope(&reader, &session("child", Some("parent")));
    assert!(result.is_err());
}
