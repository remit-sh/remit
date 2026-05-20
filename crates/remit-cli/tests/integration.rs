use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::tempdir;

fn write_fixture(dir: &Path, lines: &[&str]) -> PathBuf {
    let path = dir.join("remit.log");
    fs::write(&path, lines.join("\n") + "\n").unwrap();
    path
}

fn green(ts: &str, agent: &str, action: &str) -> String {
    format!(
        r#"{{"ts":"{ts}","event":"outcome","agent_id":"{agent}","action":"{action}","working_dir":"/tmp","verdict":"green"}}"#,
    )
}

fn red(ts: &str, agent: &str, action: &str, reason: &str) -> String {
    format!(
        r#"{{"ts":"{ts}","event":"outcome","agent_id":"{agent}","action":"{action}","working_dir":"/tmp","verdict":"red","red_kind":"OutOfScope","reason":"{reason}"}}"#,
    )
}

fn green_with_args(ts: &str, agent: &str, action: &str, args: &str) -> String {
    let escaped = args.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"ts":"{ts}","event":"outcome","agent_id":"{agent}","action":"{action}","working_dir":"/tmp","verdict":"green","args":"{escaped}"}}"#,
    )
}

#[test]
fn logs_renders_header_and_rows() {
    let dir = tempdir().unwrap();
    let log_path = write_fixture(
        dir.path(),
        &[
            &green("2026-05-12T10:14:33Z", "agent-abc", "Bash"),
            &red(
                "2026-05-12T10:14:34Z",
                "agent-abc",
                "Write",
                "writes outside scope",
            ),
        ],
    );

    let out = Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--log-path"])
        .arg(&log_path)
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.starts_with("TS\tAGENT\tACTION\tVERDICT\tREASON\n"));
    assert!(stdout.contains("agent-abc\tBash\tgreen"));
    assert!(stdout.contains("agent-abc\tWrite\tred\twrites outside scope"));
}

#[test]
fn logs_filters_by_verdict_red() {
    let dir = tempdir().unwrap();
    let log_path = write_fixture(
        dir.path(),
        &[
            &green("2026-05-12T10:14:33Z", "a", "Bash"),
            &red("2026-05-12T10:14:34Z", "a", "Write", "out of scope"),
        ],
    );

    let out = Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--verdict", "red", "--log-path"])
        .arg(&log_path)
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Write\tred"));
    assert!(!stdout.contains("Bash\tgreen"));
}

#[test]
fn logs_filters_by_action() {
    let dir = tempdir().unwrap();
    let log_path = write_fixture(
        dir.path(),
        &[
            &green("2026-05-12T10:14:33Z", "a", "Bash"),
            &green("2026-05-12T10:14:34Z", "a", "Write"),
        ],
    );

    let out = Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--action", "Write", "--log-path"])
        .arg(&log_path)
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("Write\tgreen"));
    assert!(!stdout.contains("Bash"));
}

#[test]
fn logs_missing_file_succeeds_with_empty_body() {
    let dir = tempdir().unwrap();
    let nonexistent = dir.path().join("does-not-exist.log");

    let out = Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--log-path"])
        .arg(&nonexistent)
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    // Header only, no data rows.
    assert_eq!(stdout, "TS\tAGENT\tACTION\tVERDICT\tREASON\n");
}

#[test]
fn logs_rejects_bad_verdict_value() {
    Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--verdict", "purple"])
        .assert()
        .failure();
}

#[test]
fn logs_show_args_renders_dash_when_args_absent() {
    let dir = tempdir().unwrap();
    let log_path = write_fixture(dir.path(), &[&green("2026-05-12T10:14:33Z", "a", "Bash")]);

    let out = Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--show-args", "--log-path"])
        .arg(&log_path)
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.starts_with("TS\tAGENT\tACTION\tVERDICT\tREASON\tARGS\n"));
    assert!(stdout.contains("Bash\tgreen\t\t-"));
}

#[test]
fn logs_show_args_renders_captured_args() {
    let dir = tempdir().unwrap();
    let log_path = write_fixture(
        dir.path(),
        &[&green_with_args(
            "2026-05-12T10:14:33Z",
            "a",
            "Bash",
            r#"{"command":"ls"}"#,
        )],
    );

    let out = Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--show-args", "--log-path"])
        .arg(&log_path)
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains(r#"Bash	green		{"command":"ls"}"#));
}

#[test]
fn logs_limit_returns_most_recent_n() {
    let dir = tempdir().unwrap();
    let log_path = write_fixture(
        dir.path(),
        &[
            &green("2026-05-12T10:14:30Z", "a", "First"),
            &green("2026-05-12T10:14:31Z", "a", "Second"),
            &green("2026-05-12T10:14:32Z", "a", "Third"),
            &green("2026-05-12T10:14:33Z", "a", "Fourth"),
        ],
    );

    let out = Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--limit", "2", "--log-path"])
        .arg(&log_path)
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let rows: Vec<&str> = stdout.lines().skip(1).collect();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].contains("Third"));
    assert!(rows[1].contains("Fourth"));
}

#[test]
fn logs_rejects_bad_agent_id() {
    let dir = tempdir().unwrap();
    let log_path = write_fixture(dir.path(), &[&green("2026-05-12T10:14:33Z", "a", "Bash")]);

    Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--agent", "has spaces", "--log-path"])
        .arg(&log_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid --agent"));
}

#[test]
fn logs_rejects_bad_since_duration() {
    let dir = tempdir().unwrap();
    let log_path = write_fixture(dir.path(), &[&green("2026-05-12T10:14:33Z", "a", "Bash")]);

    Command::cargo_bin("remit")
        .unwrap()
        .args(["logs", "--since", "garbage", "--log-path"])
        .arg(&log_path)
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid --since"));
}
