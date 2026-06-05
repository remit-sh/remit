use domain::{Enforced, OverrideAction, RedKind, Verdict, resolve_enforcement};

#[test]
fn green_always_permits_without_override() {
    assert_eq!(
        resolve_enforcement(&Verdict::Green, false),
        (Enforced::Permit, None),
    );
    assert_eq!(
        resolve_enforcement(&Verdict::Green, true),
        (Enforced::Permit, None),
    );
}

#[test]
fn red_denies_when_dev_bypass_off() {
    let v = Verdict::Red {
        kind: RedKind::OutOfScope,
        reason: "nope".into(),
    };
    assert_eq!(resolve_enforcement(&v, false), (Enforced::Deny, None));
}

#[test]
fn red_with_dev_bypass_permits_and_tags_dev_bypass() {
    let v = Verdict::Red {
        kind: RedKind::OutOfScope,
        reason: "nope".into(),
    };
    assert_eq!(
        resolve_enforcement(&v, true),
        (Enforced::Permit, Some(OverrideAction::DevBypass)),
    );
}
