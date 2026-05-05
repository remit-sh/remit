use domain::{Session, SessionId};

fn session(id: &str, parent: Option<&str>) -> Session {
    Session {
        id: SessionId::try_new(id).unwrap(),
        subagent_id: None,
        parent_id: parent.map(|p| SessionId::try_new(p).unwrap()),
    }
}

#[test]
fn has_parent_when_parent_distinct_from_self() {
    assert!(session("child", Some("parent")).has_parent());
}

#[test]
fn no_parent_when_parent_id_is_none() {
    assert!(!session("solo", None).has_parent());
}

#[test]
fn no_parent_when_self_referential() {
    assert!(!session("loop", Some("loop")).has_parent());
}
