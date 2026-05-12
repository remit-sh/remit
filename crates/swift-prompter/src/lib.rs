//! Rust adapter for the native Swift `remit-prompter` binary. Implements the
//! domain's [`Prompter`] port by spawning the binary per call, piping a JSON
//! request to stdin and parsing a JSON response from stdout.
//!
//! See [`docs/tn/TN-2026-011-swift-prompter.md`] for the wire protocol.
//!
//! Hard rules enforced here:
//! - 5-minute wall-clock timeout per call; expiry kills the child.
//! - 64 KiB cap on stdin write and stdout read.
//! - Non-zero exit ⇒ [`PromptError::Unavailable`] with **stderr** in the source
//!   chain. Stdout is silently discarded — it may contain user-typed scope
//!   text and must never leak into error messages.
//! - Response shape is validated against the request kind: a `confirm_disable`
//!   reply to a `choose` request fails closed.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use domain::{PromptError, Prompter, Scope, VerdictChoice};
use serde::{Deserialize, Serialize};

const PROTOCOL_VERSION: u32 = 1;
const MAX_BYTES: usize = 64 * 1024;
/// Cap stderr length included in error chains so a chatty binary can't bloat
/// the audit log on every failure.
const MAX_STDERR_IN_ERROR: usize = 4 * 1024;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[cfg(test)]
mod tests;

/// Spawns `remit-prompter` per call.
///
/// `new()` resolves the binary path lazily on each call so users can swap
/// binaries on disk without restarting the host. Tests override via
/// `with_binary` / `with_timeout`.
pub struct SwiftPrompter {
    binary_override: Option<PathBuf>,
    timeout: Duration,
}

impl SwiftPrompter {
    pub fn new() -> Self {
        Self {
            binary_override: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Override the binary path — primarily for tests that drive a fake script.
    pub fn with_binary(mut self, path: PathBuf) -> Self {
        self.binary_override = Some(path);
        self
    }

    /// Override the per-call timeout. Tests use this to exercise the
    /// hung-child path without a real 5-minute wait.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn binary_path(&self) -> PathBuf {
        if let Some(p) = &self.binary_override {
            return p.clone();
        }
        if let Ok(env_path) = std::env::var("REMIT_PROMPTER_BIN")
            && !env_path.is_empty()
        {
            return PathBuf::from(env_path);
        }
        if let Ok(self_exe) = std::env::current_exe()
            && let Some(dir) = self_exe.parent()
        {
            let sibling = dir.join("remit-prompter");
            if sibling.exists() {
                return sibling;
            }
        }
        PathBuf::from("remit-prompter")
    }
}

impl Default for SwiftPrompter {
    fn default() -> Self {
        Self::new()
    }
}

impl Prompter for SwiftPrompter {
    fn choose(
        &self,
        scope: &Scope,
        reason: &str,
        working_dir: &std::path::Path,
    ) -> Result<VerdictChoice, PromptError> {
        let request = ChooseRequest {
            protocol: PROTOCOL_VERSION,
            kind: "choose",
            scope: scope.as_ref(),
            reason,
            working_dir: &working_dir.display().to_string(),
        };
        let bytes = serde_json::to_vec(&request).map_err(unavailable)?;
        let stdout = self.run(&bytes)?;
        let response: ChooseResponse = serde_json::from_slice(&stdout).map_err(unavailable)?;
        check_kind(&response.kind, "choose")?;
        if response.cancelled == Some(true) {
            return Err(PromptError::Cancelled);
        }
        match response.choice.as_deref() {
            Some("deny") => Ok(VerdictChoice::Deny),
            Some("allow") => Ok(VerdictChoice::Allow),
            Some("update_scope") => Ok(VerdictChoice::UpdateScope),
            Some("disable_session") => Ok(VerdictChoice::DisableSession),
            Some(other) => Err(unavailable_msg(format!("unknown choice: {other}"))),
            None => Err(unavailable_msg(
                "choose response missing both `choice` and `cancelled`".into(),
            )),
        }
    }

    fn update_scope(
        &self,
        current: Option<&Scope>,
        working_dir: &std::path::Path,
    ) -> Result<Scope, PromptError> {
        let request = UpdateScopeRequest {
            protocol: PROTOCOL_VERSION,
            kind: "update_scope",
            current: current.map(Scope::as_ref),
            working_dir: &working_dir.display().to_string(),
        };
        let bytes = serde_json::to_vec(&request).map_err(unavailable)?;
        let stdout = self.run(&bytes)?;
        let response: UpdateScopeResponse = serde_json::from_slice(&stdout).map_err(unavailable)?;
        check_kind(&response.kind, "update_scope")?;
        if response.cancelled == Some(true) {
            return Err(PromptError::Cancelled);
        }
        match response.scope {
            Some(s) => {
                Scope::try_new(s).map_err(|e| unavailable_msg(format!("invalid scope: {e}")))
            }
            None => Err(unavailable_msg(
                "update_scope response missing both `scope` and `cancelled`".into(),
            )),
        }
    }

    fn confirm_disable(&self) -> Result<bool, PromptError> {
        let request = ConfirmDisableRequest {
            protocol: PROTOCOL_VERSION,
            kind: "confirm_disable",
        };
        let bytes = serde_json::to_vec(&request).map_err(unavailable)?;
        let stdout = self.run(&bytes)?;
        let response: ConfirmDisableResponse =
            serde_json::from_slice(&stdout).map_err(unavailable)?;
        check_kind(&response.kind, "confirm_disable")?;
        Ok(response.confirmed)
    }
}

#[derive(Serialize)]
struct ChooseRequest<'a> {
    protocol: u32,
    kind: &'a str,
    scope: &'a str,
    reason: &'a str,
    working_dir: &'a str,
}

#[derive(Serialize)]
struct UpdateScopeRequest<'a> {
    protocol: u32,
    kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<&'a str>,
    working_dir: &'a str,
}

#[derive(Serialize)]
struct ConfirmDisableRequest<'a> {
    protocol: u32,
    kind: &'a str,
}

/// All responses carry an explicit `kind` tag matching the request. Adapter
/// refuses any reply whose `kind` doesn't match — defends against version
/// skew silently misrouting one verdict variant into another's slot.
#[derive(Deserialize)]
struct ChooseResponse {
    kind: String,
    choice: Option<String>,
    cancelled: Option<bool>,
}

#[derive(Deserialize)]
struct UpdateScopeResponse {
    kind: String,
    scope: Option<String>,
    cancelled: Option<bool>,
}

#[derive(Deserialize)]
struct ConfirmDisableResponse {
    kind: String,
    confirmed: bool,
}

fn check_kind(actual: &str, expected: &str) -> Result<(), PromptError> {
    if actual == expected {
        Ok(())
    } else {
        Err(unavailable_msg(format!(
            "response kind mismatch: expected {expected:?}, got {actual:?}"
        )))
    }
}

impl SwiftPrompter {
    /// Spawn the binary, write `request`, wait with the configured timeout,
    /// return stdout (capped at 64 KiB). Non-zero exit ⇒ `Unavailable` with
    /// stderr in the chain; stdout is silently discarded in that case.
    fn run(&self, request: &[u8]) -> Result<Vec<u8>, PromptError> {
        if request.len() > MAX_BYTES {
            return Err(unavailable_msg(format!(
                "request exceeds {MAX_BYTES}-byte cap"
            )));
        }

        let path = self.binary_path();
        let mut child = Command::new(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| unavailable_msg(format!("failed to spawn {}: {e}", path.display())))?;

        {
            let mut stdin = child.stdin.take().expect("stdin piped");
            stdin.write_all(request).map_err(unavailable)?;
            // Dropping stdin closes the pipe and signals EOF to the child.
        }

        let start = Instant::now();
        let status = loop {
            match child.try_wait().map_err(unavailable)? {
                Some(status) => break status,
                None if start.elapsed() > self.timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(unavailable_msg(format!(
                        "prompter timed out after {:?}",
                        self.timeout
                    )));
                }
                None => std::thread::sleep(POLL_INTERVAL),
            }
        };

        // Read at most MAX_BYTES + 1 — anything more means oversize.
        let mut stdout = Vec::with_capacity(1024);
        if let Some(mut s) = child.stdout.take() {
            s.by_ref()
                .take((MAX_BYTES + 1) as u64)
                .read_to_end(&mut stdout)
                .map_err(unavailable)?;
        }
        let mut stderr = Vec::with_capacity(256);
        if let Some(mut s) = child.stderr.take() {
            s.by_ref()
                .take((MAX_BYTES + 1) as u64)
                .read_to_end(&mut stderr)
                .map_err(unavailable)?;
        }

        if stdout.len() > MAX_BYTES {
            return Err(unavailable_msg(format!(
                "prompter stdout exceeded {MAX_BYTES}-byte cap"
            )));
        }

        if !status.success() {
            if stderr.len() > MAX_STDERR_IN_ERROR {
                stderr.truncate(MAX_STDERR_IN_ERROR);
            }
            let stderr_msg = String::from_utf8_lossy(&stderr).trim().to_string();
            return Err(unavailable_msg(format!(
                "prompter exited with status {:?}: {}",
                status.code(),
                stderr_msg
            )));
        }

        Ok(stdout)
    }
}

fn unavailable<E: std::error::Error + Send + Sync + 'static>(e: E) -> PromptError {
    PromptError::Unavailable(Box::new(e))
}

fn unavailable_msg(msg: String) -> PromptError {
    PromptError::Unavailable(msg.into())
}
