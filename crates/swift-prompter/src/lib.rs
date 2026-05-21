//! Rust adapter for the native Swift `remit-prompter` binary. Implements the
//! domain's [`Prompter`] port by spawning the binary per call, piping a JSON
//! request to stdin and parsing a JSON response from stdout.
//!
//! See [`docs/tn/TN-2026-011-swift-prompter.md`] for the wire protocol and
//! [`docs/tn/TN-2026-014-swift-prompter-hang.md`] for the surface-budget
//! timeout split.
//!
//! Hard rules enforced here:
//! - **Two-phase timeout.** The adapter waits up to [`SURFACE_TIMEOUT`] for the
//!   binary either to emit `READY\n` on stderr (window is up — switch to
//!   indefinite wait) or to exit. Past the deadline with no READY and no exit,
//!   the child is killed and `Unavailable` is returned. Once READY is seen the
//!   wait is unbounded — the user may step away from their machine.
//! - 64 KiB cap on stdin write and stdout read.
//! - Non-zero exit ⇒ [`PromptError::Unavailable`] with **stderr** in the source
//!   chain. Stdout is silently discarded — it may contain user-typed scope
//!   text and must never leak into error messages.
//! - Response shape is validated against the request kind: a `confirm_disable`
//!   reply to a `choose` request fails closed.

use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use domain::{Action, PromptError, Prompter, Scope, VerdictChoice};
use serde::{Deserialize, Serialize};

const PROTOCOL_VERSION: u32 = 1;
const MAX_BYTES: usize = 64 * 1024;
/// Cap stderr length included in error chains so a chatty binary can't bloat
/// the audit log on every failure.
const MAX_STDERR_IN_ERROR: usize = 4 * 1024;
/// Surface phase deadline: how long the adapter waits for the binary to emit
/// `READY\n` on stderr (signalling the window is up). Past this, the child is
/// killed. Tuned to be long enough for cold-start AppKit init, short enough
/// that a hang doesn't freeze the hook noticeably.
const SURFACE_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Stderr line the Swift binary writes immediately after
/// `window.makeKeyAndOrderFront(nil)` and before `NSApp.runModal(for:)`. Its
/// arrival is the adapter's only signal that the UI has surfaced.
const READY_MARKER: &[u8] = b"READY";

/// Total cap on `args_preview` length. Beyond this, render as head + middle
/// indicator + tail (see [`truncate_head_tail`]).
const ARGS_PREVIEW_MAX: usize = 500;
/// Tail length retained when `args_preview` exceeds [`ARGS_PREVIEW_MAX`] —
/// preserves the destructive suffix attackers often append after harmless
/// preamble.
const ARGS_PREVIEW_TAIL: usize = 80;

#[cfg(test)]
mod tests;

/// Spawns `remit-prompter` per call.
///
/// `new()` resolves the binary path lazily on each call so users can swap
/// binaries on disk without restarting the host. Tests override via
/// `with_binary` / `with_surface_timeout`.
pub struct SwiftPrompter {
    binary_override: Option<PathBuf>,
    surface_timeout: Duration,
}

impl SwiftPrompter {
    pub fn new() -> Self {
        Self {
            binary_override: None,
            surface_timeout: SURFACE_TIMEOUT,
        }
    }

    /// Override the binary path — primarily for tests that drive a fake script.
    pub fn with_binary(mut self, path: PathBuf) -> Self {
        self.binary_override = Some(path);
        self
    }

    /// Override the surface-phase deadline. Tests use this to exercise the
    /// never-surfaces path without a real 30-second wait.
    pub fn with_surface_timeout(mut self, timeout: Duration) -> Self {
        self.surface_timeout = timeout;
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
        action: &Action,
        reason: &str,
        working_dir: &std::path::Path,
    ) -> Result<VerdictChoice, PromptError> {
        let tool_name = action.kind.as_ref();
        let args_raw = action.args.as_ref();
        let prettified = prettify_args(args_raw);
        let args_preview = truncate_head_tail(&prettified, ARGS_PREVIEW_MAX, ARGS_PREVIEW_TAIL);
        let request = ChooseRequest {
            protocol: PROTOCOL_VERSION,
            kind: "choose",
            scope: scope.as_ref(),
            reason,
            working_dir: &working_dir.display().to_string(),
            tool_name,
            args_preview: &args_preview,
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
    tool_name: &'a str,
    args_preview: &'a str,
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
    /// Spawn the binary, write `request`, wait through two phases (surface
    /// budget, then indefinite respond), return stdout capped at 64 KiB.
    /// Non-zero exit ⇒ `Unavailable` with stderr in the chain; stdout is
    /// silently discarded in that case.
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

        // Background-read stderr line by line. The thread blocks on a real
        // read; the main loop polls a non-blocking channel and so stays
        // responsive to `try_wait` + the surface deadline.
        let stderr_pipe = child.stderr.take().expect("stderr piped");
        let (tx, rx) = channel::<StderrEvent>();
        let stderr_thread = std::thread::spawn(move || read_stderr_lines(stderr_pipe, tx));

        let surface_deadline = Instant::now() + self.surface_timeout;
        let mut surfaced = false;
        let mut stderr_buf: Vec<u8> = Vec::new();

        let status = loop {
            // Drain any pending stderr events without blocking.
            while let Ok(ev) = rx.try_recv() {
                match ev {
                    StderrEvent::Ready => surfaced = true,
                    StderrEvent::Line(bytes) => append_capped(&mut stderr_buf, &bytes),
                }
            }

            if let Some(status) = child.try_wait().map_err(unavailable)? {
                break status;
            }

            if !surfaced && Instant::now() > surface_deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stderr_thread.join();
                return Err(unavailable_msg(format!(
                    "prompter did not surface within {:?}",
                    self.surface_timeout
                )));
            }

            std::thread::sleep(POLL_INTERVAL);
        };

        // Stderr thread exits naturally when the child closes the pipe on exit.
        let _ = stderr_thread.join();
        while let Ok(ev) = rx.try_recv() {
            match ev {
                StderrEvent::Ready => {} // post-exit READY is meaningless; ignore
                StderrEvent::Line(bytes) => append_capped(&mut stderr_buf, &bytes),
            }
        }

        let mut stdout = Vec::with_capacity(1024);
        if let Some(mut s) = child.stdout.take() {
            s.by_ref()
                .take((MAX_BYTES + 1) as u64)
                .read_to_end(&mut stdout)
                .map_err(unavailable)?;
        }

        if stdout.len() > MAX_BYTES {
            return Err(unavailable_msg(format!(
                "prompter stdout exceeded {MAX_BYTES}-byte cap"
            )));
        }

        if !status.success() {
            if stderr_buf.len() > MAX_STDERR_IN_ERROR {
                stderr_buf.truncate(MAX_STDERR_IN_ERROR);
            }
            let stderr_msg = String::from_utf8_lossy(&stderr_buf).trim().to_string();
            return Err(unavailable_msg(format!(
                "prompter exited with status {:?}: {}",
                status.code(),
                stderr_msg
            )));
        }

        Ok(stdout)
    }
}

/// Stderr-thread → main-loop messages. The READY marker is consumed by the
/// thread (turned into a `Ready` event) so it never appears in `stderr_buf`
/// or error chains.
enum StderrEvent {
    Ready,
    Line(Vec<u8>),
}

fn read_stderr_lines(pipe: std::process::ChildStderr, tx: Sender<StderrEvent>) {
    let mut reader = std::io::BufReader::new(pipe);
    let mut buf = Vec::new();
    // Cumulative non-READY bytes sent to the main loop. Past [`MAX_BYTES`] we
    // keep reading (so the child can't block on a full pipe) but stop emitting
    // `Line` events — caps the channel's in-flight queue against a chatty or
    // malicious binary.
    let mut bytes_sent: usize = 0;
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break, // EOF or read failure
            Ok(_) => {}
        }
        let trimmed = strip_line_ending(&buf);
        let event = if trimmed == READY_MARKER {
            StderrEvent::Ready
        } else if bytes_sent <= MAX_BYTES {
            bytes_sent = bytes_sent.saturating_add(buf.len());
            StderrEvent::Line(buf.clone())
        } else {
            continue; // past cap — keep draining the pipe, drop the line
        };
        if tx.send(event).is_err() {
            break; // receiver dropped
        }
    }
}

fn strip_line_ending(bytes: &[u8]) -> &[u8] {
    let without_lf = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    without_lf.strip_suffix(b"\r").unwrap_or(without_lf)
}

/// Append `bytes` to `buf`, refusing to grow `buf` beyond [`MAX_BYTES`] + 1.
/// Keeps the audit-log error chain bounded for a chatty binary.
fn append_capped(buf: &mut Vec<u8>, bytes: &[u8]) {
    let remaining = (MAX_BYTES + 1).saturating_sub(buf.len());
    let take = remaining.min(bytes.len());
    buf.extend_from_slice(&bytes[..take]);
}

fn unavailable<E: std::error::Error + Send + Sync + 'static>(e: E) -> PromptError {
    PromptError::Unavailable(Box::new(e))
}

fn unavailable_msg(msg: String) -> PromptError {
    PromptError::Unavailable(msg.into())
}

/// Strip JSON noise from a tool-input args string for dialog display, without
/// any per-tool knowledge. Parses `args` as JSON; if it's a non-empty object,
/// renders `key: value, key: value` with string values unquoted. Nested objects
/// and arrays stay as compact JSON. Anything that doesn't parse as a non-empty
/// object falls through as the raw input.
///
/// Tool-agnostic by design — security review (#44) flagged that picking a
/// single "primary" field is gameable by malicious input. Showing *all*
/// top-level fields removes that vector: nothing can hide behind an
/// alphabetically-earlier sibling.
fn prettify_args(args: &str) -> String {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(args) else {
        return args.to_string();
    };
    if map.is_empty() {
        return args.to_string();
    }
    map.into_iter()
        .map(|(k, v)| format!("{}: {}", k, render_value(&v)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render a JSON value for the prettified args row. Strings come out unquoted;
/// everything else falls back to compact JSON so structure is preserved for
/// nested objects/arrays.
fn render_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

/// Truncate `s` to at most `max` chars by `char` boundaries. When over the cap,
/// emit `<head> … [truncated] … <tail>` so the destructive tail of pathological
/// inputs (long heredocs, base64 blobs with a command appended) remains visible.
fn truncate_head_tail(s: &str, max: usize, tail: usize) -> String {
    let total = s.chars().count();
    if total <= max {
        return s.to_string();
    }
    let head = max.saturating_sub(tail);
    let head_str: String = s.chars().take(head).collect();
    let tail_str: String = s.chars().skip(total - tail).collect();
    format!("{head_str} … [truncated] … {tail_str}")
}
