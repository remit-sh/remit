//! Append-only NDJSON classification history. One JSON object per line; an
//! `event_kind` discriminator distinguishes `"classification"` records from
//! `"lifecycle"` and `"state-transition"` events.
//!
//! Writer is **best-effort** (ADR-0002 logging carve-out): IO errors are
//! swallowed and the methods return `Ok(())`. The port shape returns
//! `Result<(), HistoryError>` so #38's SQLite adapter can flip fail-closed
//! without changing call sites.
//!
//! Cross-process atomicity: each `write` / `write_event` holds an exclusive
//! advisory lock for one `write_all` syscall (TN-2026-006 pattern).

use std::collections::HashMap;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use domain::{
    ActionKind, AgentId, AgentType, CallId, ClassificationEvent, ClassificationHistoryReader,
    ClassificationHistoryWriter, ClassificationRecord, CorrelationId, Enforced, HistoryError,
    HistoryQuery, HistoryQueryError, OverrideAction, RawVerdict, RecordedClassifierOutcome,
    RecordedLlmCallStats, RedKind, Scope, ScopeOrigin, Verdict, VerdictFilter,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub struct FileClassificationHistoryWriter {
    path: PathBuf,
}

impl FileClassificationHistoryWriter {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn write_line<T: Serialize>(&self, value: &T) -> Result<(), HistoryError> {
        let Ok(mut json) = serde_json::to_string(value) else {
            return Ok(());
        };
        json.push('\n');
        if let Some(parent) = self.path.parent() {
            let _ = create_dir_all(parent);
        }
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return Ok(());
        };
        // Audit log holds operational data (scope text, reasons, optionally
        // tool_input). Enforce 0600 on every write so a permissive umask can't
        // leave the row world-readable. Best-effort: a chmod failure doesn't
        // block the write.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
        }
        if file.lock().is_err() {
            return Ok(());
        }
        let _ = file.write_all(json.as_bytes());
        Ok(())
    }
}

impl ClassificationHistoryWriter for FileClassificationHistoryWriter {
    fn write(&self, record: ClassificationRecord) -> Result<(), HistoryError> {
        let wire = ClassificationWire::from_record(&record);
        self.write_line(&wire)
    }

    fn write_event(&self, event: ClassificationEvent) -> Result<(), HistoryError> {
        let wire = EventWire::from_event(&event);
        self.write_line(&wire)
    }
}

#[derive(Serialize, Deserialize)]
struct ClassificationWire {
    event_kind: String,
    ts: String,
    session_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    agent_type: Option<String>,
    working_dir: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    call_id: Option<String>,
    correlation_id: Uuid,
    pid: u32,
    hook_duration_ms: u128,
    tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    tool_input: Option<String>,
    bundled_scope_present: bool,
    narration_fields_stripped: u32,
    args_size_pre: u32,
    args_size_post: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    scope_origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    llm: Option<LlmWire>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    raw_verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    raw_reason: Option<String>,
    prompt_shown: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    prompt_latency_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    lock_wait_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    initial_verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    initial_red_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    initial_reason: Option<String>,
    verdict: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    red_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    reason: Option<String>,
    enforced: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    override_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    error_chain: Option<String>,
    dev_bypass: bool,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty", default)]
    details: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct LlmWire {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    model: Option<String>,
    latency_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    output_tokens: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct EventWire {
    event_kind: String,
    ts: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    correlation_id: Option<Uuid>,
    message: String,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty", default)]
    details: std::collections::BTreeMap<String, String>,
    dev_bypass: bool,
}

impl ClassificationWire {
    fn from_record(r: &ClassificationRecord) -> Self {
        let (verdict_str, red_kind_str, reason_str) = verdict_columns(&r.verdict);
        let (initial_verdict_str, initial_red_kind_str, initial_reason_str) =
            match &r.initial_verdict {
                Some(v) => {
                    let (a, b, c) = verdict_columns(v);
                    (Some(a.to_string()), b.map(|s| s.to_string()), c)
                }
                None => (None, None, None),
            };
        Self {
            event_kind: "classification".to_string(),
            ts: r.ts.format(&Rfc3339).unwrap_or_default(),
            session_id: r.session_id.as_ref().to_string(),
            parent_id: r.parent_id.as_ref().map(|p| p.as_ref().to_string()),
            agent_type: r.agent_type.as_ref().map(|t| t.as_ref().to_string()),
            working_dir: r.working_dir.display().to_string(),
            call_id: r.call_id.as_ref().map(|c| c.as_ref().to_string()),
            correlation_id: *r.correlation_id.as_uuid(),
            pid: r.pid,
            hook_duration_ms: r.hook_duration.as_millis(),
            tool_name: r.tool_name.as_ref().to_string(),
            tool_input: r.tool_input.clone(),
            bundled_scope_present: r.bundled_scope_present,
            narration_fields_stripped: r.narration_fields_stripped,
            args_size_pre: r.args_size_pre,
            args_size_post: r.args_size_post,
            scope: r.scope.as_ref().map(|s| s.as_ref().to_string()),
            scope_origin: r.scope_origin.map(|o| o.as_str().to_string()),
            llm: r.llm_outcome.as_ref().and_then(|o| {
                o.llm.as_ref().map(|l| LlmWire {
                    model: Some(l.model.clone()),
                    latency_ms: l.latency.as_millis(),
                    input_tokens: l.input_tokens,
                    output_tokens: l.output_tokens,
                })
            }),
            raw_verdict: r
                .llm_outcome
                .as_ref()
                .map(|o| o.raw_verdict.as_str().to_string()),
            raw_reason: r.llm_outcome.as_ref().and_then(|o| o.raw_reason.clone()),
            prompt_shown: r.prompt_shown,
            prompt_latency_ms: r.prompt_latency.map(|d| d.as_millis()),
            lock_wait_ms: r.lock_wait.map(|d| d.as_millis()),
            initial_verdict: initial_verdict_str,
            initial_red_kind: initial_red_kind_str,
            initial_reason: initial_reason_str,
            verdict: verdict_str.to_string(),
            red_kind: red_kind_str.map(|s| s.to_string()),
            reason: reason_str,
            enforced: r.enforced.as_str().to_string(),
            override_action: r.override_action.map(|o| o.as_str().to_string()),
            error_chain: r.error_chain.clone(),
            dev_bypass: r.dev_bypass,
            details: r.details.clone(),
        }
    }

    fn into_record(self) -> Result<ClassificationRecord, HistoryQueryError> {
        let ts = OffsetDateTime::parse(&self.ts, &Rfc3339).map_err(|e| {
            HistoryQueryError::Unavailable(Box::new(parse_err(format!(
                "invalid ts {:?}: {e}",
                self.ts
            ))))
        })?;
        let session_id = AgentId::try_new(self.session_id.clone()).map_err(|_| {
            HistoryQueryError::Unavailable(Box::new(parse_err(format!(
                "invalid session_id {:?}",
                self.session_id
            ))))
        })?;
        let parent_id = match self.parent_id {
            Some(p) => Some(AgentId::try_new(p.clone()).map_err(|_| {
                HistoryQueryError::Unavailable(Box::new(parse_err(format!(
                    "invalid parent_id {p:?}"
                ))))
            })?),
            None => None,
        };
        let agent_type = match self.agent_type {
            Some(t) => Some(AgentType::try_new(t.clone()).map_err(|_| {
                HistoryQueryError::Unavailable(Box::new(parse_err(format!(
                    "invalid agent_type {t:?}"
                ))))
            })?),
            None => None,
        };
        let call_id = self.call_id.map(CallId::new);
        let scope = match self.scope {
            Some(s) => Some(Scope::try_new(s.clone()).map_err(|_| {
                HistoryQueryError::Unavailable(Box::new(parse_err(format!("invalid scope {s:?}"))))
            })?),
            None => None,
        };
        let scope_origin = match self.scope_origin.as_deref() {
            Some("pre-existing") => Some(ScopeOrigin::PreExisting),
            Some("claimed-from-pending") => Some(ScopeOrigin::ClaimedFromPending),
            Some("fresh") => Some(ScopeOrigin::Fresh),
            Some("edited") => Some(ScopeOrigin::Edited),
            Some("extracted") => Some(ScopeOrigin::Extracted),
            Some(other) => {
                return Err(HistoryQueryError::Unavailable(Box::new(parse_err(
                    format!("unknown scope_origin {other:?}"),
                ))));
            }
            None => None,
        };
        let verdict = parse_verdict(&self.verdict, self.red_kind.as_deref(), self.reason)?;
        let initial_verdict = match self.initial_verdict.as_deref() {
            Some(v) => Some(parse_verdict(
                v,
                self.initial_red_kind.as_deref(),
                self.initial_reason,
            )?),
            None => None,
        };
        let enforced = match self.enforced.as_str() {
            "permit" => Enforced::Permit,
            "deny" => Enforced::Deny,
            other => {
                return Err(HistoryQueryError::Unavailable(Box::new(parse_err(
                    format!("unknown enforced {other:?}"),
                ))));
            }
        };
        let override_action = match self.override_action.as_deref() {
            Some("user-allow") => Some(OverrideAction::UserAllow),
            Some("user-deny") => Some(OverrideAction::UserDeny),
            Some("update-scope") => Some(OverrideAction::UpdateScope),
            Some("disable-session") => Some(OverrideAction::DisableSession),
            Some("dev-bypass") => Some(OverrideAction::DevBypass),
            Some(other) => {
                return Err(HistoryQueryError::Unavailable(Box::new(parse_err(
                    format!("unknown override_action {other:?}"),
                ))));
            }
            None => None,
        };
        let raw_verdict = match self.raw_verdict.as_deref() {
            Some("green") => Some(RawVerdict::Green),
            Some("red") => Some(RawVerdict::Red),
            Some(other) => {
                return Err(HistoryQueryError::Unavailable(Box::new(parse_err(
                    format!("unknown raw_verdict {other:?}"),
                ))));
            }
            None => None,
        };
        let llm_outcome = raw_verdict.map(|rv| RecordedClassifierOutcome {
            raw_verdict: rv,
            raw_reason: self.raw_reason,
            llm: self.llm.map(|l| RecordedLlmCallStats {
                model: l.model.unwrap_or_default(),
                latency: Duration::from_millis(u128_to_u64(l.latency_ms)),
                input_tokens: l.input_tokens,
                output_tokens: l.output_tokens,
            }),
        });
        Ok(ClassificationRecord {
            ts,
            session_id,
            parent_id,
            agent_type,
            working_dir: PathBuf::from(self.working_dir),
            call_id,
            correlation_id: CorrelationId::from_uuid(self.correlation_id),
            pid: self.pid,
            hook_duration: Duration::from_millis(u128_to_u64(self.hook_duration_ms)),
            tool_name: ActionKind::new(self.tool_name),
            tool_input: self.tool_input,
            bundled_scope_present: self.bundled_scope_present,
            narration_fields_stripped: self.narration_fields_stripped,
            args_size_pre: self.args_size_pre,
            args_size_post: self.args_size_post,
            scope,
            scope_origin,
            llm_outcome,
            prompt_shown: self.prompt_shown,
            prompt_latency: self
                .prompt_latency_ms
                .map(|ms| Duration::from_millis(u128_to_u64(ms))),
            lock_wait: self
                .lock_wait_ms
                .map(|ms| Duration::from_millis(u128_to_u64(ms))),
            initial_verdict,
            verdict,
            enforced,
            override_action,
            error_chain: self.error_chain,
            dev_bypass: self.dev_bypass,
            details: self.details,
        })
    }
}

fn u128_to_u64(v: u128) -> u64 {
    if v > u64::MAX as u128 {
        u64::MAX
    } else {
        v as u64
    }
}

impl EventWire {
    fn from_event(e: &ClassificationEvent) -> Self {
        Self {
            event_kind: e.kind.as_str().to_string(),
            ts: e.ts.format(&Rfc3339).unwrap_or_default(),
            session_id: e.session_id.as_ref().map(|s| s.as_ref().to_string()),
            correlation_id: e.correlation_id.as_ref().map(|c| *c.as_uuid()),
            message: e.message.clone(),
            details: e.details.clone(),
            dev_bypass: e.dev_bypass,
        }
    }
}

fn verdict_columns(v: &Verdict) -> (&'static str, Option<&'static str>, Option<String>) {
    match v {
        Verdict::Green => ("Green", None, None),
        Verdict::Red { kind, reason } => ("Red", Some(red_kind_label(kind)), Some(reason.clone())),
    }
}

fn parse_verdict(
    label: &str,
    red_kind: Option<&str>,
    reason: Option<String>,
) -> Result<Verdict, HistoryQueryError> {
    match label {
        "Green" => Ok(Verdict::Green),
        "Red" => {
            let kind = red_kind.and_then(red_kind_from_label).ok_or_else(|| {
                HistoryQueryError::Unavailable(Box::new(parse_err(format!(
                    "unknown red_kind {red_kind:?}"
                ))))
            })?;
            Ok(Verdict::Red {
                kind,
                reason: reason.unwrap_or_default(),
            })
        }
        other => Err(HistoryQueryError::Unavailable(Box::new(parse_err(
            format!("unknown verdict {other:?}"),
        )))),
    }
}

fn red_kind_label(kind: &RedKind) -> &'static str {
    match kind {
        RedKind::NoScopeRoot => "NoScopeRoot",
        RedKind::ScopeSetupCancelled => "ScopeSetupCancelled",
        RedKind::ScopeSetupUnavailable => "ScopeSetupUnavailable",
        RedKind::SubagentMissingScope => "SubagentMissingScope",
        RedKind::StoreUnavailable => "StoreUnavailable",
        RedKind::OutOfScope => "OutOfScope",
        RedKind::ClassifierUnavailable => "ClassifierUnavailable",
        RedKind::StandingGuidanceUnavailable => "StandingGuidanceUnavailable",
    }
}

fn red_kind_from_label(s: &str) -> Option<RedKind> {
    Some(match s {
        "NoScopeRoot" => RedKind::NoScopeRoot,
        "ScopeSetupCancelled" => RedKind::ScopeSetupCancelled,
        "ScopeSetupUnavailable" => RedKind::ScopeSetupUnavailable,
        "SubagentMissingScope" => RedKind::SubagentMissingScope,
        "StoreUnavailable" => RedKind::StoreUnavailable,
        "OutOfScope" => RedKind::OutOfScope,
        "ClassifierUnavailable" => RedKind::ClassifierUnavailable,
        "StandingGuidanceUnavailable" => RedKind::StandingGuidanceUnavailable,
        _ => return None,
    })
}

#[derive(thiserror::Error, Debug)]
#[error("malformed log line at line {line}: {message}")]
struct LineParseError {
    line: usize,
    message: String,
}

fn parse_err(message: String) -> LineParseError {
    LineParseError { line: 0, message }
}

/// File-backed read adapter for the [`ClassificationHistoryReader`] port.
pub struct FileClassificationHistoryReader {
    path: PathBuf,
}

impl FileClassificationHistoryReader {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ClassificationHistoryReader for FileClassificationHistoryReader {
    fn query(&self, query: &HistoryQuery) -> Result<Vec<ClassificationRecord>, HistoryQueryError> {
        let content = match read_locked(&self.path) {
            Ok(Some(s)) => s,
            Ok(None) => return Ok(Vec::new()),
            Err(e) => return Err(HistoryQueryError::Unavailable(Box::new(e))),
        };

        let mut parent_map: HashMap<String, String> = HashMap::new();
        let mut classifications: Vec<ClassificationWire> = Vec::new();

        // Skip-and-log malformed rows rather than poisoning the whole query.
        // A single corrupted / hand-edited line should not make `remit logs`
        // unusable. Stderr surfaces the line number for operator follow-up.
        for (idx, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("remit logs: skipping malformed line {}: {e}", idx + 1);
                    continue;
                }
            };
            let event_kind = value.get("event_kind").and_then(|v| v.as_str());
            if event_kind != Some("classification") {
                continue;
            }
            if let (Some(child), Some(parent)) = (
                value.get("session_id").and_then(|v| v.as_str()),
                value.get("parent_id").and_then(|v| v.as_str()),
            ) {
                parent_map
                    .entry(child.to_string())
                    .or_insert_with(|| parent.to_string());
            }
            let wire: ClassificationWire = match serde_json::from_value(value) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("remit logs: skipping malformed line {}: {e}", idx + 1);
                    continue;
                }
            };
            classifications.push(wire);
        }

        let mut materialised: Vec<ClassificationRecord> = Vec::with_capacity(classifications.len());
        for wire in classifications {
            match wire.into_record() {
                Ok(r) => materialised.push(r),
                Err(e) => {
                    eprintln!("remit logs: skipping line: {e}");
                    continue;
                }
            }
        }

        let filtered: Vec<ClassificationRecord> = materialised
            .into_iter()
            .filter(|r| query_matches(r, query, &parent_map))
            .collect();
        Ok(apply_limit(filtered, query.limit))
    }
}

fn read_locked(path: &PathBuf) -> std::io::Result<Option<String>> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let _ = file.lock_shared();
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(Some(buf))
}

fn query_matches(
    record: &ClassificationRecord,
    query: &HistoryQuery,
    parent_map: &HashMap<String, String>,
) -> bool {
    if let Some(agent_id) = &query.agent_id
        && &record.session_id != agent_id
    {
        return false;
    }
    if let Some(root_agent_id) = &query.root_agent_id
        && !is_descendant_of(
            record.session_id.as_ref(),
            root_agent_id.as_ref(),
            parent_map,
        )
    {
        return false;
    }
    if let Some(filter) = query.verdict
        && !verdict_matches(filter, &record.verdict)
    {
        return false;
    }
    if let Some(since) = query.since
        && SystemTime::from(record.ts) < since
    {
        return false;
    }
    if let Some(action) = &query.action
        && &record.tool_name != action
    {
        return false;
    }
    true
}

fn verdict_matches(filter: VerdictFilter, verdict: &Verdict) -> bool {
    matches!(
        (filter, verdict),
        (VerdictFilter::Green, Verdict::Green) | (VerdictFilter::Red, Verdict::Red { .. })
    )
}

fn is_descendant_of(agent: &str, root: &str, parent_map: &HashMap<String, String>) -> bool {
    if agent == root {
        return true;
    }
    let mut visited = std::collections::HashSet::new();
    let mut current = agent.to_string();
    while visited.insert(current.clone()) {
        match parent_map.get(&current) {
            Some(parent) if parent == root => return true,
            Some(parent) => current = parent.clone(),
            None => return false,
        }
    }
    false
}

fn apply_limit(
    mut records: Vec<ClassificationRecord>,
    limit: Option<usize>,
) -> Vec<ClassificationRecord> {
    match limit {
        Some(n) if n > 0 && records.len() > n => records.split_off(records.len() - n),
        _ => records,
    }
}
