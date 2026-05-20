use std::time::Duration;

use crate::since;

#[test]
fn since_parses_simple_minutes() {
    let d = since::parse("10m").unwrap();
    assert_eq!(d, Duration::from_secs(600));
}

#[test]
fn since_parses_hours_and_minutes() {
    let d = since::parse("1h30m").unwrap();
    assert_eq!(d, Duration::from_secs(5400));
}

#[test]
fn since_parses_seconds() {
    let d = since::parse("30s").unwrap();
    assert_eq!(d, Duration::from_secs(30));
}

#[test]
fn since_rejects_garbage() {
    let err = since::parse("not-a-duration").unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("not-a-duration"), "msg surfaces input: {msg}");
}
