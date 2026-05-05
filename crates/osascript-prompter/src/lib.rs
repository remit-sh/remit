use std::process::{Command, Output};

use domain::{PromptError, Scope, ScopePrompter, SessionId};

#[cfg(test)]
mod tests;

const SCRIPT: &str = r#"display dialog "Set scope for this session" default answer "" with title "Remit" buttons {"Cancel", "Set scope"} default button "Set scope""#;

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
    fn prompt(&self, _session_id: &SessionId) -> Result<Scope, PromptError> {
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
    let answer = extract_text_returned(&stdout).ok_or_else(|| {
        PromptError::Unavailable(format!("unexpected osascript output: {}", stdout.trim()).into())
    })?;
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return Err(PromptError::Cancelled);
    }
    Ok(Scope::new(trimmed.to_string()))
}

fn extract_text_returned(stdout: &str) -> Option<&str> {
    stdout
        .split_once("text returned:")
        .map(|(_, after)| after.trim_end_matches('\n'))
}
