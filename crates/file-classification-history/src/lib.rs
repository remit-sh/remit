use std::collections::HashMap;
use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use domain::{
    ActionArgs, ActionKind, AgentId, ClassificationHistoryReader, ClassificationHistoryWriter,
    ClassificationInput, ClassificationRecord, HistoryQuery, HistoryQueryError, RedKind, Verdict,
    VerdictFilter,
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

#[cfg(test)]
mod tests;

/// Append-only NDJSON classification history. Every line is a JSON
/// object. Classifier records are written via the typed
/// [`ClassificationHistoryWriter`] methods; adapter-specific events
/// (hook lifecycle, bundled-scope handoffs, anything the writer doesn't
/// know about) flow through [`ClassificationHistoryWriter::record_custom`].
///
/// Best-effort: IO errors are swallowed so a logging glitch never
/// fails-open. Writes are atomic across processes: an exclusive
/// advisory lock is held for the duration of one `write_all` call,
/// and the JSON line (with trailing newline) is built as a single
/// buffer so the syscall is one shot. This prevents interleaving when
/// many writer processes append concurrently.
///
/// `markers` describe persistent audit-trail labels (e.g.
/// `dev-bypass-active`, `debug-mode`) and appear as a field on every
/// record.
///
/// `debug_mode` controls whether records carry the action's `args`. Off
/// by default — args may contain sensitive content (CLAUDE.md). On for
/// forensic scans.
#[derive(Clone)]
pub struct FileClassificationHistoryWriter {
    path: PathBuf,
    markers: Vec<String>,
    debug_mode: bool,
}

impl FileClassificationHistoryWriter {
    pub fn new(path: PathBuf, markers: Vec<String>, debug_mode: bool) -> Self {
        Self {
            path,
            markers,
            debug_mode,
        }
    }

    fn write_record<T: Serialize>(&self, record: &T) {
        let Ok(mut json) = serde_json::to_string(record) else {
            return;
        };
        json.push('\n');
        self.write_raw(&json);
    }

    fn write_raw(&self, buf: &str) {
        if let Some(parent) = self.path.parent() {
            let _ = create_dir_all(parent);
        }
        let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        else {
            return;
        };
        // Block until we hold the exclusive lock. If locking fails, skip
        // the write — best-effort logging never fails-open.
        if file.lock().is_err() {
            return;
        }
        let _ = file.write_all(buf.as_bytes());
        // File drop releases the lock.
    }

    fn now_rfc3339(&self) -> String {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::new())
    }

    fn args_for_record<'a>(&self, input: &'a ClassificationInput) -> Option<&'a str> {
        self.debug_mode.then(|| input.action.args.as_ref())
    }
}

impl ClassificationHistoryWriter for FileClassificationHistoryWriter {
    fn record_incoming(&self, input: &ClassificationInput) {
        let record = IncomingRecord {
            ts: &self.now_rfc3339(),
            event: "incoming",
            agent_id: input.agent.id.as_ref(),
            parent_id: input.agent.parent_id.as_ref().map(|id| id.as_ref()),
            call_id: input.action.call_id.as_ref().map(|id| id.as_ref()),
            action: input.action.kind.as_ref(),
            working_dir: input.working_dir.display().to_string(),
            args: self.args_for_record(input),
            markers: &self.markers,
        };
        self.write_record(&record);
    }

    fn record_outcome(&self, input: &ClassificationInput, verdict: &Verdict) {
        let (verdict_label, red_kind, reason) = match verdict {
            Verdict::Green => ("green", None, None),
            Verdict::Red { kind, reason } => {
                ("red", Some(red_kind_label(kind)), Some(reason.as_str()))
            }
        };
        let record = OutcomeRecord {
            ts: &self.now_rfc3339(),
            event: "outcome",
            agent_id: input.agent.id.as_ref(),
            parent_id: input.agent.parent_id.as_ref().map(|id| id.as_ref()),
            action: input.action.kind.as_ref(),
            working_dir: input.working_dir.display().to_string(),
            verdict: verdict_label,
            red_kind,
            reason,
            args: self.args_for_record(input),
            markers: &self.markers,
        };
        self.write_record(&record);
    }

    fn record_error(&self, input: &ClassificationInput, error: &(dyn std::error::Error + 'static)) {
        let mut chain = error.to_string();
        let mut current = error.source();
        while let Some(c) = current {
            chain.push_str(": ");
            chain.push_str(&c.to_string());
            current = c.source();
        }
        let record = ErrorRecord {
            ts: &self.now_rfc3339(),
            event: "error",
            agent_id: input.agent.id.as_ref(),
            chain: &chain,
            markers: &self.markers,
        };
        self.write_record(&record);
    }

    fn record_event(&self, input: &ClassificationInput, event: &str) {
        let record = EventRecord {
            ts: &self.now_rfc3339(),
            event: "event",
            agent_id: input.agent.id.as_ref(),
            name: event,
            markers: &self.markers,
        };
        self.write_record(&record);
    }

    fn record_hook_event(&self, name: &str) {
        let record = HookRecord {
            ts: &self.now_rfc3339(),
            event: "hook",
            name,
            markers: &self.markers,
        };
        self.write_record(&record);
    }

    fn record_custom(&self, kind: &str, payload: serde_json::Value) {
        let mut obj = serde_json::Map::new();
        obj.insert("ts".into(), serde_json::Value::String(self.now_rfc3339()));
        obj.insert("event".into(), serde_json::Value::String(kind.to_string()));
        if let serde_json::Value::Object(payload_obj) = payload {
            for (k, v) in payload_obj {
                obj.insert(k, v);
            }
        }
        if !self.markers.is_empty() {
            obj.insert(
                "markers".into(),
                serde_json::Value::Array(
                    self.markers
                        .iter()
                        .cloned()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
        let Ok(mut json) = serde_json::to_string(&serde_json::Value::Object(obj)) else {
            return;
        };
        json.push('\n');
        self.write_raw(&json);
    }
}

#[derive(Serialize)]
struct IncomingRecord<'a> {
    ts: &'a str,
    event: &'static str,
    agent_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    call_id: Option<&'a str>,
    action: &'a str,
    working_dir: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<&'a str>,
    #[serde(skip_serializing_if = "is_empty_str_vec")]
    markers: &'a [String],
}

#[derive(Serialize)]
struct OutcomeRecord<'a> {
    ts: &'a str,
    event: &'static str,
    agent_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_id: Option<&'a str>,
    action: &'a str,
    working_dir: String,
    verdict: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    red_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<&'a str>,
    #[serde(skip_serializing_if = "is_empty_str_vec")]
    markers: &'a [String],
}

#[derive(Serialize)]
struct ErrorRecord<'a> {
    ts: &'a str,
    event: &'static str,
    agent_id: &'a str,
    chain: &'a str,
    #[serde(skip_serializing_if = "is_empty_str_vec")]
    markers: &'a [String],
}

#[derive(Serialize)]
struct EventRecord<'a> {
    ts: &'a str,
    event: &'static str,
    agent_id: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "is_empty_str_vec")]
    markers: &'a [String],
}

#[derive(Serialize)]
struct HookRecord<'a> {
    ts: &'a str,
    event: &'static str,
    name: &'a str,
    #[serde(skip_serializing_if = "is_empty_str_vec")]
    markers: &'a [String],
}

fn is_empty_str_vec(v: &[String]) -> bool {
    v.is_empty()
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

/// File-backed read adapter for the [`ClassificationHistoryReader`]
/// port. Reads the same NDJSON file that
/// [`FileClassificationHistoryWriter`] writes; parses on demand.
///
/// Two-pass: pass 1 builds the parent-agent chain from both `incoming`
/// and `outcome` records (covers parents that spawned children before
/// issuing any tool calls of their own); pass 2 materialises
/// [`ClassificationRecord`]s from `outcome` lines and applies the
/// query filters in memory.
///
/// Fail-closed posture matches the rest of the security tool: a
/// malformed line yields [`HistoryQueryError::Unavailable`]. A missing
/// file is **not** an error — fresh installs return `Ok(vec![])`.
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
        let mut outcomes: Vec<OutcomeWire> = Vec::new();
        for (idx, line) in content.lines().enumerate() {
            ingest_line(line, idx + 1, &mut parent_map, &mut outcomes)?;
        }

        let mut materialised: Vec<ClassificationRecord> = Vec::with_capacity(outcomes.len());
        for (idx, wire) in outcomes.into_iter().enumerate() {
            materialised.push(wire.into_record(idx + 1)?);
        }

        let filtered: Vec<ClassificationRecord> = materialised
            .into_iter()
            .filter(|r| query_matches(r, query, &parent_map))
            .collect();
        Ok(apply_limit(filtered, query.limit))
    }
}

fn ingest_line(
    line: &str,
    line_no: usize,
    parent_map: &mut HashMap<String, String>,
    outcomes: &mut Vec<OutcomeWire>,
) -> Result<(), HistoryQueryError> {
    if line.trim().is_empty() {
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_str(line).map_err(|e| {
        HistoryQueryError::Unavailable(Box::new(LineParseError {
            line: line_no,
            message: e.to_string(),
        }))
    })?;
    let event = value.get("event").and_then(|v| v.as_str());
    if !matches!(event, Some("incoming") | Some("outcome")) {
        return Ok(());
    }
    if let (Some(child), Some(parent)) = (
        value.get("agent_id").and_then(|v| v.as_str()),
        value.get("parent_id").and_then(|v| v.as_str()),
    ) {
        parent_map
            .entry(child.to_string())
            .or_insert_with(|| parent.to_string());
    }
    if event == Some("outcome") {
        let wire: OutcomeWire = serde_json::from_value(value).map_err(|e| {
            HistoryQueryError::Unavailable(Box::new(LineParseError {
                line: line_no,
                message: e.to_string(),
            }))
        })?;
        outcomes.push(wire);
    }
    Ok(())
}

fn read_locked(path: &PathBuf) -> std::io::Result<Option<String>> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    // Cooperate with the writer's exclusive lock. Best-effort: if the
    // shared lock can't be acquired we still attempt the read — a
    // momentary interleave is acceptable for query output.
    let _ = file.lock_shared();
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    Ok(Some(buf))
}

#[derive(thiserror::Error, Debug)]
#[error("malformed log line at line {line}: {message}")]
struct LineParseError {
    line: usize,
    message: String,
}

#[derive(Deserialize)]
struct OutcomeWire {
    ts: String,
    agent_id: String,
    #[serde(default)]
    parent_id: Option<String>,
    action: String,
    verdict: String,
    #[serde(default)]
    red_kind: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    working_dir: String,
    #[serde(default)]
    args: Option<String>,
}

impl OutcomeWire {
    fn into_record(self, line_no: usize) -> Result<ClassificationRecord, HistoryQueryError> {
        let parsed_ts = OffsetDateTime::parse(&self.ts, &Rfc3339).map_err(|e| {
            HistoryQueryError::Unavailable(Box::new(LineParseError {
                line: line_no,
                message: format!("invalid ts {:?}: {e}", self.ts),
            }))
        })?;
        let agent_id = AgentId::try_new(self.agent_id.clone()).map_err(|_| {
            HistoryQueryError::Unavailable(Box::new(LineParseError {
                line: line_no,
                message: format!("invalid agent_id {:?}", self.agent_id),
            }))
        })?;
        let parent_id = match self.parent_id {
            Some(p) => Some(AgentId::try_new(p.clone()).map_err(|_| {
                HistoryQueryError::Unavailable(Box::new(LineParseError {
                    line: line_no,
                    message: format!("invalid parent_id {p:?}"),
                }))
            })?),
            None => None,
        };
        let verdict = match self.verdict.as_str() {
            "green" => Verdict::Green,
            "red" => {
                let kind = self
                    .red_kind
                    .as_deref()
                    .and_then(red_kind_from_label)
                    .ok_or_else(|| {
                        HistoryQueryError::Unavailable(Box::new(LineParseError {
                            line: line_no,
                            message: format!("unknown red_kind {:?}", self.red_kind),
                        }))
                    })?;
                let reason = self.reason.unwrap_or_default();
                Verdict::Red { kind, reason }
            }
            other => {
                return Err(HistoryQueryError::Unavailable(Box::new(LineParseError {
                    line: line_no,
                    message: format!("unknown verdict {other:?}"),
                })));
            }
        };
        Ok(ClassificationRecord {
            timestamp: SystemTime::from(parsed_ts),
            agent_id,
            parent_agent_id: parent_id,
            action: ActionKind::new(self.action),
            verdict,
            working_dir: PathBuf::from(self.working_dir),
            args: self.args.map(ActionArgs::new),
        })
    }
}

fn query_matches(
    record: &ClassificationRecord,
    query: &HistoryQuery,
    parent_map: &HashMap<String, String>,
) -> bool {
    if let Some(agent_id) = &query.agent_id
        && &record.agent_id != agent_id
    {
        return false;
    }
    if let Some(root_agent_id) = &query.root_agent_id
        && !is_descendant_of(record.agent_id.as_ref(), root_agent_id.as_ref(), parent_map)
    {
        return false;
    }
    if let Some(filter) = query.verdict
        && !verdict_matches(filter, &record.verdict)
    {
        return false;
    }
    if let Some(since) = query.since
        && record.timestamp < since
    {
        return false;
    }
    if let Some(action) = &query.action
        && &record.action != action
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
    let mut current = agent;
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    while seen.insert(current) {
        match parent_map.get(current) {
            Some(p) if p == root => return true,
            Some(p) => current = p.as_str(),
            None => return false,
        }
    }
    false
}

fn apply_limit(
    mut records: Vec<ClassificationRecord>,
    limit: Option<usize>,
) -> Vec<ClassificationRecord> {
    if let Some(n) = limit.filter(|&n| n > 0)
        && records.len() > n
    {
        let total = records.len();
        records.drain(..total - n);
    }
    records
}
