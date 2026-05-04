use super::*;

#[test]
fn no_env_means_no_parent() {
    assert_eq!(
        infer_parent_id(None, &SessionId::try_new("self".to_string()).unwrap()),
        None
    );
}

#[test]
fn env_differing_from_self_is_parent() {
    let env = SessionId::try_new("parent-id".to_string()).unwrap();
    let me = SessionId::try_new("self".to_string()).unwrap();
    assert_eq!(infer_parent_id(Some(env.clone()), &me), Some(env));
}

#[test]
fn env_equal_to_self_is_ignored() {
    let me = SessionId::try_new("self".to_string()).unwrap();
    assert_eq!(infer_parent_id(Some(me.clone()), &me), None);
}
