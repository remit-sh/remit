//! Host-side configuration for Remit.
//!
//! Loads `<remit_home>/config.toml`. Missing file → defaults. Malformed file
//! or unknown fields → error (callers fail-closed). Domain config will join
//! here as a `[domain]` section once it appears.
//!
//! `remit_home` resolves to `$REMIT_HOME` if set and non-empty, else
//! `$HOME/.remit`.

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
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

/// Load the user's config from `<remit_home>/config.toml`.
pub fn load() -> Result<RemitConfig, ConfigError> {
    let path = remit_home()?.join("config.toml");
    load_from(&path)
}

/// Directory where bound agent scopes are persisted: `<remit_home>/scopes/`.
pub fn scope_dir() -> Result<PathBuf, ConfigError> {
    Ok(remit_home()?.join("scopes"))
}

/// Directory where parent-keyed pending scopes (staged for the next-spawned
/// child) are persisted: `<remit_home>/pending-scopes/`.
pub fn pending_scope_dir() -> Result<PathBuf, ConfigError> {
    Ok(remit_home()?.join("pending-scopes"))
}

/// Path to the audit log: `<remit_home>/remit.log`.
pub fn log_path() -> Result<PathBuf, ConfigError> {
    Ok(remit_home()?.join("remit.log"))
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
        Ok(text) => Ok(toml::from_str(&text)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(RemitConfig::default()),
        Err(e) => Err(ConfigError::Io(e)),
    }
}
