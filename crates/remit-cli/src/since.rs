//! `--since` duration parser. Accepts the humantime grammar: `10m`,
//! `1h30m`, `30s`, `2d`, etc. Returns a `Duration` for the caller to
//! subtract from `now()`.

use std::time::Duration;

#[derive(thiserror::Error, Debug)]
pub enum SinceError {
    #[error("invalid --since duration {value:?}: {message}")]
    Invalid { value: String, message: String },
}

pub fn parse(s: &str) -> Result<Duration, SinceError> {
    humantime::parse_duration(s).map_err(|e| SinceError::Invalid {
        value: s.to_string(),
        message: e.to_string(),
    })
}
