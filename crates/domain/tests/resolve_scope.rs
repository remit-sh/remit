use domain::{Scope, Session, SessionId, resolve_scope};

mod common;
use common::FakeReader;

fn session(id: &str, parent: Option<&str>) -> Session {
    Session {
        id: SessionId::try_new(id).unwrap(),
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
fn does_not_fall_back_to_parent_scope() {
    let parent_id = SessionId::try_new("parent").unwrap();
    let reader = FakeReader::new().with(parent_id, Scope::new("parent task"));
    // Subagent has no own scope; parent has one. We refuse to inherit:
    // narrowed children must not silently run under broader parent scopes.
    let got = resolve_scope(&reader, &session("child", Some("parent"))).unwrap();
    assert_eq!(got, None);
}

#[test]
fn returns_none_when_session_has_no_scope_row() {
    let reader = FakeReader::new();
    let got = resolve_scope(&reader, &session("child", Some("parent"))).unwrap();
    assert_eq!(got, None);
}

#[test]
fn returns_none_for_root_with_no_scope() {
    let reader = FakeReader::new();
    let got = resolve_scope(&reader, &session("root", None)).unwrap();
    assert_eq!(got, None);
}

#[test]
fn propagates_reader_error() {
    let id = SessionId::try_new("child").unwrap();
    let reader = FakeReader::new().failing_for(id);
    let result = resolve_scope(&reader, &session("child", None));
    assert!(result.is_err());
}
