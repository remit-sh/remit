use domain::Scope;

#[test]
fn rejects_empty_scope() {
    assert!(Scope::try_new("").is_err());
}

#[test]
fn rejects_whitespace_only_scope() {
    assert!(Scope::try_new("   ").is_err());
    assert!(Scope::try_new("\n\t  ").is_err());
}

#[test]
fn accepts_non_empty_scope() {
    assert!(Scope::try_new("do the thing").is_ok());
}

#[test]
fn accepts_scope_with_leading_whitespace() {
    // A real prompt may start with whitespace. We only reject when *every* char is whitespace.
    assert!(Scope::try_new("  leading whitespace is fine").is_ok());
}
