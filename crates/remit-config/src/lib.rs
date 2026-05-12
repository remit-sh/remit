//! Host-side configuration for Remit.
//!
//! Loads `<remit_home>/config.toml`. Missing file → defaults. Malformed file
//! or unknown fields → error (callers fail-closed). Domain config will join
//! here as a `[domain]` section once it appears.
//!
//! `remit_home` resolves to `$REMIT_HOME` if set and non-empty, else
//! `$HOME/.remit`.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[cfg(test)]
mod tests;

/// Top-level Remit configuration.
#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RemitConfig {
    /// When true, the hook runs the classifier as normal but exits 0 even on
    /// `Verdict::Red`. The would-have-denied reason is logged to stderr.
    pub dev_bypass: bool,
    pub classifier: ClassifierConfig,
}

/// Classifier-side configuration.
///
/// `adapter` selects which classifier the composition root wires; each
/// supported adapter has its own typed sub-section. New adapters are added
/// here as new variants on [`ClassifierAdapter`] + new fields on this struct.
#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct ClassifierConfig {
    /// Which classifier adapter to use. Defaults to `anthropic`.
    pub adapter: ClassifierAdapter,
    /// Settings consumed when `adapter = "anthropic"`. Ignored otherwise.
    pub anthropic: AnthropicConfig,
}

/// Selector for the classifier adapter the host wires. Exhaustive on purpose:
/// adding a new variant forces the composition root's match to grow a new arm.
#[derive(Deserialize, Debug, PartialEq, Eq, Default, Clone, Copy)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum ClassifierAdapter {
    #[default]
    Anthropic,
}

/// Anthropic adapter configuration. All fields optional — the adapter resolves
/// defaults (model) and env-var fallbacks (api_key) at construction time.
#[derive(Deserialize, Default, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct AnthropicConfig {
    /// Anthropic model id, e.g. `claude-haiku-4-5`. `None` → adapter default.
    pub model: Option<String>,
    /// Anthropic API key. `None` → adapter falls back to `ANTHROPIC_API_KEY` env var.
    pub api_key: Option<String>,
}

/// Custom `Debug` redacts `api_key` so a careless `dbg!(cfg)` can't leak the key.
impl fmt::Debug for AnthropicConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnthropicConfig")
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl RemitConfig {
    /// Audit-log markers active for this config — flags whose state the audit
    /// trail should reflect. Empty when no marker-bearing flag is set.
    pub fn audit_markers(&self) -> Vec<String> {
        let mut markers = Vec::new();
        if self.dev_bypass {
            markers.push("[dev-bypass-active]".into());
        }
        markers
    }
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("neither REMIT_HOME nor HOME environment variable is set")]
    HomeNotSet,
    #[error("failed to read config file: {0}")]
    Io(#[from] io::Error),
    /// Parse failure. Carries only the type-level message — the offending TOML
    /// line is *not* included because it would echo the surrounding values
    /// (e.g. `api_key = "sk-..."`) to stderr / logs on schema mismatches.
    #[error("failed to parse config: {0}")]
    Parse(String),
}

/// Convert a `toml::de::Error` to a `ConfigError::Parse`, redacting the line
/// snippet (which can contain secrets) and keeping only the structural message.
fn parse_error(e: toml::de::Error) -> ConfigError {
    ConfigError::Parse(redact_toml_error(&e))
}

fn redact_toml_error(e: &toml::de::Error) -> String {
    e.message().to_string()
}

/// Load the user's config from `<remit_home>/config.toml`. Emits a stderr
/// warning (best-effort) if the file is group/other-readable and contains an
/// API key — `chmod 600` is recommended for any host with shared accounts.
pub fn load() -> Result<RemitConfig, ConfigError> {
    let path = remit_home()?.join("config.toml");
    let cfg = load_from(&path)?;
    warn_if_permissive(&path, &cfg);
    Ok(cfg)
}

#[cfg(unix)]
fn warn_if_permissive(path: &Path, cfg: &RemitConfig) {
    use std::os::unix::fs::PermissionsExt;
    if !contains_secret(cfg) {
        return;
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    let mode = meta.permissions().mode();
    if mode & 0o077 != 0 {
        eprintln!(
            "remit: warning: {} is mode {:o} but contains an adapter API key; recommend `chmod 600`",
            path.display(),
            mode & 0o777,
        );
    }
}

#[cfg(not(unix))]
fn warn_if_permissive(_path: &Path, _cfg: &RemitConfig) {}

/// Whether any adapter section in `cfg` contains a secret that warrants the
/// permissive-mode warning. Centralised so adding a new adapter in [`ClassifierConfig`]
/// flags exactly one place to update.
fn contains_secret(cfg: &RemitConfig) -> bool {
    cfg.classifier.anthropic.api_key.is_some()
}

/// Path to the audit log: `<remit_home>/remit.log`.
pub fn log_path() -> Result<PathBuf, ConfigError> {
    Ok(remit_home()?.join("remit.log"))
}

/// Single root for all agent-keyed state: `<remit_home>/agents/`. Per-agent
/// state lives under `<agents_dir>/<agent-id>/` as a small set of files
/// (`scope.json`, `pending.json`, `disabled.marker`, `prompt.lock`).
/// Adapters that operate on agent state (`FileAgentStore`, `FsPromptLock`)
/// take this directory and resolve their files inside it.
pub fn agents_dir() -> Result<PathBuf, ConfigError> {
    Ok(remit_home()?.join("agents"))
}

/// Resolve the Remit home directory: `$REMIT_HOME` if set and non-empty,
/// else `$HOME/.remit`.
fn remit_home() -> Result<PathBuf, ConfigError> {
    remit_home_from(std::env::var("REMIT_HOME").ok(), std::env::var("HOME").ok())
}

fn remit_home_from(
    remit_home_env: Option<String>,
    home_env: Option<String>,
) -> Result<PathBuf, ConfigError> {
    if let Some(rh) = remit_home_env.filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(rh));
    }
    home_env
        .filter(|s| !s.is_empty())
        .map(|h| PathBuf::from(h).join(".remit"))
        .ok_or(ConfigError::HomeNotSet)
}

/// Load config from an explicit path. Missing file returns defaults.
pub fn load_from(path: &Path) -> Result<RemitConfig, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).map_err(parse_error),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(RemitConfig::default()),
        Err(e) => Err(ConfigError::Io(e)),
    }
}
