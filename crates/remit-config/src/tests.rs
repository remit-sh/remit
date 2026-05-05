use std::fs;

use tempfile::tempdir;

use super::*;

#[test]
fn defaults_when_file_missing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("nope.toml");
    let cfg = load_from(&path).unwrap();
    assert_eq!(cfg, RemitConfig::default());
    assert!(!cfg.dev_bypass);
}

#[test]
fn parses_dev_bypass_true() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "dev_bypass = true").unwrap();
    let cfg = load_from(&path).unwrap();
    assert!(cfg.dev_bypass);
}

#[test]
fn parses_dev_bypass_false() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "dev_bypass = false").unwrap();
    let cfg = load_from(&path).unwrap();
    assert!(!cfg.dev_bypass);
}

#[test]
fn defaults_when_field_omitted() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "").unwrap();
    let cfg = load_from(&path).unwrap();
    assert!(!cfg.dev_bypass);
}

#[test]
fn defaults_when_only_whitespace() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "   \n\n# a comment\n").unwrap();
    let cfg = load_from(&path).unwrap();
    assert!(!cfg.dev_bypass);
}

#[test]
fn errors_on_malformed_toml() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "dev_bypass = not_a_bool").unwrap();
    assert!(matches!(load_from(&path), Err(ConfigError::Parse(_))));
}

#[test]
fn rejects_unknown_fields() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "dev_bypas = true").unwrap();
    assert!(matches!(load_from(&path), Err(ConfigError::Parse(_))));
}

#[test]
fn errors_on_directory_path() {
    let dir = tempdir().unwrap();
    let result = load_from(dir.path());
    assert!(matches!(result, Err(ConfigError::Io(_))));
}

#[test]
fn scope_dir_under_home() {
    let got = remit_subdir_from_home(Some("/tmp/fake-home".into()), "scopes").unwrap();
    assert_eq!(
        got,
        std::path::PathBuf::from("/tmp/fake-home/.remit/scopes")
    );
}

#[test]
fn pending_scope_dir_under_home() {
    let got = remit_subdir_from_home(Some("/tmp/fake-home".into()), "pending-scopes").unwrap();
    assert_eq!(
        got,
        std::path::PathBuf::from("/tmp/fake-home/.remit/pending-scopes")
    );
}

#[test]
fn scope_dir_errors_when_home_missing() {
    assert!(matches!(
        remit_subdir_from_home(None, "scopes"),
        Err(ConfigError::HomeNotSet)
    ));
}
