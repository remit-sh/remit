use std::process::{Command, Output};

use domain::{AgentId, PromptError, Scope, ScopePrompter};

#[cfg(test)]
mod tests;

const SCRIPT: &str = r#"display dialog "Set scope for this session" default answer "" with title "Remit" buttons {"Cancel", "Set scope"} default button "Set scope""#;

/// macOS-only [`ScopePrompter`]: pops a native dialog via `osascript` and reads
/// the typed answer. Cancel/empty → `Cancelled`; spawn-or-IO failure → `Unavailable`.
pub struct OsascriptPrompter;

impl OsascriptPrompter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OsascriptPrompter {
    fn default() -> Self {
        Self::new()
    }
}

impl ScopePrompter for OsascriptPrompter {
    fn prompt(&self, _agent_id: &AgentId) -> Result<Scope, PromptError> {
        let output = Command::new("osascript")
            .arg("-e")
            .arg(SCRIPT)
            .output()
            .map_err(|e| PromptError::Unavailable(Box::new(e)))?;
        parse_osascript_output(&output)
    }
}

fn parse_osascript_output(output: &Output) -> Result<Scope, PromptError> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("User canceled") {
            return Err(PromptError::Cancelled);
        }
        return Err(PromptError::Unavailable(
            format!(
                "osascript exited unsuccessfully (status={:?}): {}",
                output.status.code(),
                stderr.trim()
            )
            .into(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Never embed `stdout` in the error: on success it contains the user's typed
    // scope (after the `text returned:` marker), and the error chain is persisted
    // by `ClassifierLogger::log_error`.
    let answer = extract_text_returned(&stdout)
        .ok_or_else(|| PromptError::Unavailable("unexpected osascript output format".into()))?;
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return Err(PromptError::Cancelled);
    }
    Scope::try_new(trimmed.to_string())
        .map_err(|e| PromptError::Unavailable(format!("invalid scope: {e}").into()))
}

fn extract_text_returned(stdout: &str) -> Option<&str> {
    stdout
        .split_once("text returned:")
        .map(|(_, after)| after.trim_end_matches('\n'))
}
