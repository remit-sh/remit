use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use domain::{Action, PromptError, Prompter, Scope, VerdictChoice};

/// Remit symbol (square icon, no text). Embedded at compile time and written
/// to a tempfile so AppleScript can `initWithContentsOfFile:` it. NSAlert's
/// icon slot clamps to a square area; we pair this with `messageText:"Remit"`
/// to put the brand name right of the icon, where the wordmark couldn't fit.
const ICON_BYTES: &[u8] = include_bytes!("../assets/remit-icon.png");

#[cfg(test)]
mod tests;

/// macOS-only [`Prompter`]: drives all user-facing dialogs via `osascript`.
/// Three dialogs:
/// - [`choose`] — multi-button verdict (Deny / Allow / Update scope / Disable session).
/// - [`update_scope`] — text input for setting / replacing the agent's [`Scope`].
/// - [`confirm_disable`] — yes/no confirm before disabling the session.
///
/// IO failures map to [`PromptError::Unavailable`]; user cancellations to
/// [`PromptError::Cancelled`]. Stdout from a successful run is never embedded
/// in error chains — it can contain user-typed scope text.
///
/// [`choose`]: Prompter::choose
/// [`update_scope`]: Prompter::update_scope
/// [`confirm_disable`]: Prompter::confirm_disable
pub struct OsascriptPrompter {
    icon_path: PathBuf,
}

impl OsascriptPrompter {
    pub fn new() -> Self {
        let icon_path = std::env::temp_dir().join("remit-icon.png");
        // Best-effort: a missing or unreadable icon falls back to NSAlert's
        // default (which is fine — slightly uglier dialog, not broken).
        if !icon_path.exists() {
            let _ = std::fs::write(&icon_path, ICON_BYTES);
        }
        Self { icon_path }
    }
}

impl Default for OsascriptPrompter {
    fn default() -> Self {
        Self::new()
    }
}

const CHOOSE_DENY: &str = "Deny";
const CHOOSE_ALLOW: &str = "Allow";
const CHOOSE_UPDATE_SCOPE: &str = "Update scope";
const CHOOSE_DISABLE_SESSION: &str = "Disable session";

const CONFIRM_DISABLE: &str = "Disable";

impl Prompter for OsascriptPrompter {
    fn choose(
        &self,
        scope: &Scope,
        _action: &Action,
        reason: &str,
        working_dir: &Path,
    ) -> Result<VerdictChoice, PromptError> {
        // NSAlert (via AppleScript-ObjC) — supports >3 buttons and our Remit
        // icon, unlike `display dialog`. Buttons render right-to-left from the
        // right side; Deny is first so it's both the rightmost default and
        // the safest fallback. Esc-without-a-Cancel-button is a no-op in
        // NSAlert; user must pick deliberately.
        let body = format!(
            "{cwd}\n\nReason: {reason}\n\nScope: {scope}",
            cwd = display_dir(working_dir),
            reason = reason,
            scope = scope.as_ref(),
        );
        let script = format!(
            r#"use framework "Foundation"
use framework "AppKit"
use scripting additions

set theAlert to current application's NSAlert's alloc()'s init()
theAlert's setMessageText:"Remit"
theAlert's setInformativeText:{body}
set theIcon to current application's NSImage's alloc()'s initWithContentsOfFile:{icon_path}
theIcon's setSize:(current application's NSMakeSize(64, 64))
theAlert's setIcon:theIcon

-- Deny is the default; Return activates. Others get single-key shortcuts;
-- letter shown in the label so the binding is discoverable.
theAlert's addButtonWithTitle:{deny}
set allowBtn to theAlert's addButtonWithTitle:{allow_label}
allowBtn's setKeyEquivalent:"a"
set updateBtn to theAlert's addButtonWithTitle:{update_label}
updateBtn's setKeyEquivalent:"u"
set disableBtn to theAlert's addButtonWithTitle:{disable_label}
disableBtn's setKeyEquivalent:"s"

-- Wide spacer accessoryView forces NSAlert wider, which keeps the 4 buttons
-- on a single row instead of stacking vertically.
set spacer to current application's NSView's alloc()'s initWithFrame:(current application's NSMakeRect(0, 0, 480, 1))
theAlert's setAccessoryView:spacer

current application's NSApp's setActivationPolicy:1
current application's NSApp's activateIgnoringOtherApps:true
set r to theAlert's runModal() as integer

if r is 1000 then
    return {deny}
else if r is 1001 then
    return {allow}
else if r is 1002 then
    return {update}
else if r is 1003 then
    return {disable}
end if
return "false""#,
            body = applescript_string(&body),
            icon_path = applescript_string(&self.icon_path.to_string_lossy()),
            deny = applescript_string(CHOOSE_DENY),
            allow = applescript_string(CHOOSE_ALLOW),
            update = applescript_string(CHOOSE_UPDATE_SCOPE),
            disable = applescript_string(CHOOSE_DISABLE_SESSION),
            allow_label = applescript_string(&format!("{CHOOSE_ALLOW} (a)")),
            update_label = applescript_string(&format!("{CHOOSE_UPDATE_SCOPE} (u)")),
            disable_label = applescript_string(&format!("{CHOOSE_DISABLE_SESSION} (s)")),
        );
        let output = run_osascript(&script)?;
        parse_choose_output(&output)
    }

    fn update_scope(
        &self,
        current: Option<&Scope>,
        working_dir: &Path,
    ) -> Result<Scope, PromptError> {
        let lead = if current.is_some() {
            "Update the scope for this session."
        } else {
            "Set scope for this session."
        };
        let body = format!("{cwd}\n\n{lead}", cwd = display_dir(working_dir));
        let default = current.map(|s| s.as_ref()).unwrap_or("");
        // AppleScript-ObjC: NSAlert with NSTextView accessory gives a multi-line
        // input. `display dialog` is single-line and capped in size; this is the
        // standard way to get a proper text area in pure osascript.
        let script = format!(
            r#"use framework "Foundation"
use framework "AppKit"
use scripting additions

set theAlert to current application's NSAlert's alloc()'s init()
theAlert's setMessageText:"Remit"
theAlert's setInformativeText:{body}
theAlert's addButtonWithTitle:"Save"
theAlert's addButtonWithTitle:"Cancel"
set theIcon to current application's NSImage's alloc()'s initWithContentsOfFile:{icon_path}
theIcon's setSize:(current application's NSMakeSize(64, 64))
theAlert's setIcon:theIcon

set f to current application's NSMakeRect(0, 0, 400, 80)
set sv to current application's NSScrollView's alloc()'s initWithFrame:f
sv's setHasVerticalScroller:true
sv's setBorderType:(current application's NSBezelBorder)

set tv to current application's NSTextView's alloc()'s initWithFrame:f
tv's setEditable:true
tv's setRichText:false
tv's setFont:(current application's NSFont's userFixedPitchFontOfSize:13)
tv's setString:{default}
sv's setDocumentView:tv

theAlert's setAccessoryView:sv

current application's NSApp's setActivationPolicy:1
current application's NSApp's activateIgnoringOtherApps:true
set r to theAlert's runModal() as integer
if r is not 1000 then
    return "text returned:"
end if

return "text returned:" & ((tv's |string|()) as text)"#,
            body = applescript_string(&body),
            default = applescript_string(default),
            icon_path = applescript_string(&self.icon_path.to_string_lossy()),
        );
        let output = run_osascript(&script)?;
        parse_update_scope_output(&output)
    }

    fn confirm_disable(&self) -> Result<bool, PromptError> {
        let script = format!(
            r#"display dialog {body} with title "Remit" buttons {{"Cancel", "{disable}"}} default button "Cancel""#,
            body = applescript_string(
                "Disable Remit for the rest of this session?\n\nClassification will be skipped until the Claude Code session ends."
            ),
            disable = CONFIRM_DISABLE,
        );
        let output = run_osascript(&script)?;
        parse_confirm_disable_output(&output)
    }
}

/// Render a path for dialog display: tilde-collapse `$HOME` to `~` so paths
/// fit the prompt area and read at a glance.
fn display_dir(path: &Path) -> String {
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        let rest = rest.display().to_string();
        if rest.is_empty() {
            return "~".into();
        }
        return format!("~/{rest}");
    }
    path.display().to_string()
}

fn run_osascript(script: &str) -> Result<Output, PromptError> {
    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| PromptError::Unavailable(Box::new(e)))
}

/// Quote a string for embedding inside an AppleScript literal. Wraps in `"`,
/// escapes embedded `"` and `\`. Newlines become `\n` (AppleScript uses the same
/// escape).
fn applescript_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn user_cancelled(stderr: &str) -> bool {
    stderr.contains("User canceled")
}

fn parse_choose_output(output: &Output) -> Result<VerdictChoice, PromptError> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if user_cancelled(&stderr) {
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
    // `choose from list` returns either the selected item text or the literal
    // `false` on cancel. Selection prints `<item>\n`; cancel prints `false\n`.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim_end_matches('\n').trim();
    if trimmed == "false" {
        return Err(PromptError::Cancelled);
    }
    match trimmed {
        CHOOSE_DENY => Ok(VerdictChoice::Deny),
        CHOOSE_ALLOW => Ok(VerdictChoice::Allow),
        CHOOSE_UPDATE_SCOPE => Ok(VerdictChoice::UpdateScope),
        CHOOSE_DISABLE_SESSION => Ok(VerdictChoice::DisableSession),
        other => Err(PromptError::Unavailable(
            format!("unexpected list selection: {other}").into(),
        )),
    }
}

fn parse_update_scope_output(output: &Output) -> Result<Scope, PromptError> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if user_cancelled(&stderr) {
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
    // Never embed `stdout` in error chains: on success it carries the user's
    // typed scope (after the `text returned:` marker).
    let answer = extract_text_returned(&stdout)
        .ok_or_else(|| PromptError::Unavailable("unexpected osascript output format".into()))?;
    let trimmed = answer.trim();
    if trimmed.is_empty() {
        return Err(PromptError::Cancelled);
    }
    Scope::try_new(trimmed.to_string())
        .map_err(|e| PromptError::Unavailable(format!("invalid scope: {e}").into()))
}

fn parse_confirm_disable_output(output: &Output) -> Result<bool, PromptError> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if user_cancelled(&stderr) {
            return Ok(false);
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
    let button = extract_button_returned(&stdout)
        .ok_or_else(|| PromptError::Unavailable("unexpected osascript output format".into()))?;
    match button.trim() {
        CONFIRM_DISABLE => Ok(true),
        "Cancel" => Ok(false),
        other => Err(PromptError::Unavailable(
            format!("unexpected button returned: {other}").into(),
        )),
    }
}

fn extract_text_returned(stdout: &str) -> Option<&str> {
    stdout
        .split_once("text returned:")
        .map(|(_, after)| after.trim_end_matches('\n'))
}

fn extract_button_returned(stdout: &str) -> Option<&str> {
    // osascript returns either `button returned:Name` (single-line) or, when both
    // a button and text are returned (text-input dialogs), `button returned:Name, text returned:...`.
    let after = stdout.split_once("button returned:")?.1;
    Some(match after.split_once(", text returned:") {
        Some((button, _)) => button,
        None => after.trim_end_matches('\n'),
    })
}
