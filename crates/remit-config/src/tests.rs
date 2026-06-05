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
fn parses_anthropic_classifier_section() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[classifier]\nadapter = \"anthropic\"\n[classifier.anthropic]\nmodel = \"claude-haiku-4-5\"\napi_key = \"sk-ant-test\"\n",
    )
    .unwrap();
    let cfg = load_from(&path).unwrap();
    assert_eq!(cfg.classifier.adapter, ClassifierAdapter::Anthropic);
    assert_eq!(
        cfg.classifier.anthropic.model.as_deref(),
        Some("claude-haiku-4-5")
    );
    assert_eq!(
        cfg.classifier.anthropic.api_key.as_deref(),
        Some("sk-ant-test")
    );
}

#[test]
fn classifier_adapter_defaults_to_anthropic() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[classifier.anthropic]\napi_key = \"sk-ant-test\"\n").unwrap();
    let cfg = load_from(&path).unwrap();
    assert_eq!(cfg.classifier.adapter, ClassifierAdapter::Anthropic);
}

#[test]
fn classifier_section_optional_fields_default_to_none() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[classifier.anthropic]\n").unwrap();
    let cfg = load_from(&path).unwrap();
    assert!(cfg.classifier.anthropic.model.is_none());
    assert!(cfg.classifier.anthropic.api_key.is_none());
}

#[test]
fn classifier_section_omitted_means_default() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "dev_bypass = true").unwrap();
    let cfg = load_from(&path).unwrap();
    assert_eq!(cfg.classifier, ClassifierConfig::default());
}

#[test]
fn rejects_unknown_classifier_field() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[classifier]\nmysterious = \"x\"\n").unwrap();
    assert!(matches!(load_from(&path), Err(ConfigError::Parse(_))));
}

#[test]
fn rejects_unknown_anthropic_field() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[classifier.anthropic]\nmodle = \"x\"\n").unwrap();
    assert!(matches!(load_from(&path), Err(ConfigError::Parse(_))));
}

#[test]
fn parse_error_does_not_echo_secret_values() {
    // Regression: `toml::de::Error::Display` includes a snippet of the offending
    // line, which would echo `api_key = "sk-..."` to stderr on a schema mismatch.
    // We strip the snippet down to the structural message before emitting.
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let secret = "sk-ant-secret-must-not-leak-XYZ123";
    fs::write(&path, format!("[classifier]\napi_key = \"{secret}\"\n")).unwrap();
    let err = load_from(&path).unwrap_err();
    let displayed = format!("{err}");
    assert!(
        !displayed.contains(secret),
        "parse error must not echo secret values; got: {displayed}"
    );
    // The structural reason should still be present.
    assert!(
        displayed.contains("api_key") || displayed.contains("unknown field"),
        "parse error should still describe the structural problem; got: {displayed}"
    );
}

#[test]
fn rejects_unknown_adapter_value() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[classifier]\nadapter = \"unknown-adapter\"\n").unwrap();
    assert!(matches!(load_from(&path), Err(ConfigError::Parse(_))));
}

#[test]
fn debug_redacts_anthropic_api_key() {
    let cfg = AnthropicConfig {
        model: Some("claude-haiku-4-5".into()),
        api_key: Some("sk-ant-secret-do-not-print".into()),
    };
    let debug = format!("{cfg:?}");
    assert!(!debug.contains("sk-ant-secret-do-not-print"));
    assert!(debug.contains("<redacted>"));
    assert!(debug.contains("claude-haiku-4-5"));
}

#[test]
fn debug_shows_none_for_missing_anthropic_api_key() {
    let cfg = AnthropicConfig {
        model: None,
        api_key: None,
    };
    let debug = format!("{cfg:?}");
    assert!(debug.contains("None"));
    assert!(!debug.contains("<redacted>"));
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
fn parses_debug_mode_true() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "debug_mode = true").unwrap();
    let cfg = load_from(&path).unwrap();
    assert!(cfg.debug_mode);
}

#[test]
fn defaults_debug_mode_to_false() {
    assert!(!RemitConfig::default().debug_mode);
}
