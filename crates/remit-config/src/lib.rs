//! Host-side configuration for Remit.
//!
//! Loads `~/.remit/config.toml`. Missing file → defaults. Malformed file or
//! unknown fields → error (callers fail-closed). Domain config will join here
//! as a `[domain]` section once it appears.

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

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum ConfigError {
    #[error("HOME environment variable is not set")]
    HomeNotSet,
    #[error("failed to read config file: {0}")]
    Io(#[from] io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

/// Load the user's config from `~/.remit/config.toml`.
pub fn load() -> Result<RemitConfig, ConfigError> {
    let path = default_path()?;
    load_from(&path)
}

/// Directory where session scopes are persisted: `~/.remit/scopes/`.
pub fn scope_dir() -> Result<PathBuf, ConfigError> {
    scope_dir_from_home(std::env::var("HOME").ok())
}

fn scope_dir_from_home(home: Option<String>) -> Result<PathBuf, ConfigError> {
    home.map(|h| PathBuf::from(h).join(".remit").join("scopes"))
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

fn default_path() -> Result<PathBuf, ConfigError> {
    let home = std::env::var("HOME").map_err(|_| ConfigError::HomeNotSet)?;
    Ok(PathBuf::from(home).join(".remit").join("config.toml"))
}
