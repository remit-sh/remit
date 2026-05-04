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
