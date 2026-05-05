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
fn remit_home_defaults_to_home_dot_remit() {
    let got = remit_home_from(None, Some("/tmp/fake-home".into())).unwrap();
    assert_eq!(got, std::path::PathBuf::from("/tmp/fake-home/.remit"));
}

#[test]
fn remit_home_env_overrides_home() {
    let got = remit_home_from(Some("/custom/remit".into()), Some("/tmp/fake-home".into())).unwrap();
    assert_eq!(got, std::path::PathBuf::from("/custom/remit"));
}

#[test]
fn empty_remit_home_falls_back_to_home() {
    let got = remit_home_from(Some(String::new()), Some("/tmp/fake-home".into())).unwrap();
    assert_eq!(got, std::path::PathBuf::from("/tmp/fake-home/.remit"));
}

#[test]
fn empty_home_with_no_remit_home_errors() {
    assert!(matches!(
        remit_home_from(None, Some(String::new())),
        Err(ConfigError::HomeNotSet)
    ));
}

#[test]
fn errors_when_neither_env_var_set() {
    assert!(matches!(
        remit_home_from(None, None),
        Err(ConfigError::HomeNotSet)
    ));
}

#[test]
fn audit_markers_empty_by_default() {
    assert!(RemitConfig::default().audit_markers().is_empty());
}

#[test]
fn audit_markers_includes_dev_bypass_when_set() {
    let cfg = RemitConfig { dev_bypass: true };
    assert_eq!(cfg.audit_markers(), vec!["[dev-bypass-active]".to_string()]);
}
