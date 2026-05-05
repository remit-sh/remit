use super::*;

#[test]
fn no_env_means_no_parent() {
    assert_eq!(
        parent_from_env_value(None, &SessionId::try_new("self".to_string()).unwrap()),
        None
    );
}

#[test]
fn env_differing_from_self_is_parent() {
    let me = SessionId::try_new("self".to_string()).unwrap();
    assert_eq!(
        parent_from_env_value(Some("parent-id".into()), &me),
        Some(SessionId::try_new("parent-id".to_string()).unwrap())
    );
}

#[test]
fn env_equal_to_self_is_ignored() {
    let me = SessionId::try_new("self".to_string()).unwrap();
    assert_eq!(parent_from_env_value(Some("self".into()), &me), None);
}

#[test]
fn env_with_invalid_id_is_ignored() {
    let me = SessionId::try_new("self".to_string()).unwrap();
    assert_eq!(
        parent_from_env_value(Some("not a valid id".into()), &me),
        None
    );
}

#[test]
fn parses_double_quoted_sentinel() {
    let got = parse_prepare_sentinel(r#"remit prepare-next-scope "edit cargo files""#);
    assert_eq!(got, Some("edit cargo files".to_string()));
}

#[test]
fn parses_single_quoted_sentinel() {
    let got = parse_prepare_sentinel("remit prepare-next-scope 'one task'");
    assert_eq!(got, Some("one task".to_string()));
}

#[test]
fn parses_bare_word_sentinel() {
    let got = parse_prepare_sentinel("remit prepare-next-scope explore-readme");
    assert_eq!(got, Some("explore-readme".to_string()));
}

#[test]
fn ignores_non_sentinel_command() {
    assert_eq!(parse_prepare_sentinel("ls -la"), None);
    assert_eq!(parse_prepare_sentinel("remit-something else"), None);
}

#[test]
fn ignores_sentinel_without_arg() {
    assert_eq!(parse_prepare_sentinel("remit prepare-next-scope"), None);
    assert_eq!(parse_prepare_sentinel("remit prepare-next-scope   "), None);
}

#[test]
fn truncates_at_terminator_for_bare_arg() {
    let got = parse_prepare_sentinel("remit prepare-next-scope foo && echo ok");
    assert_eq!(got, Some("foo".to_string()));
}
