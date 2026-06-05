---
number: 2026-016
title: Rich classification record + single-write history port
kind: plan
status: active
issue: "#56"
tags: [domain, history, classifier, refactor, foundation]
---

# TN-2026-016: Rich classification record + single-write history port

> Replace the 6-method incremental `ClassificationHistoryWriter` with single `write(record)` + `write_event(event)`. `process_action` builds one rich `ClassificationRecord` (~25 fields: identity, LLM call, scope provenance, prompt timing, layered verdict, override action) and writes once at end. Eager `write_event` for state transitions. Enrich `LlmClassifier::classify` to `ClassifierOutcome`. Rewrite NDJSON `file-classification-history` to new port. Adapt `remit-cli` reader. Foundation for SQLite (#38).

## Goals

- One audit row per `process_action` call, written at end.
- Forensic depth: LLM telemetry, scope provenance, prompt timing, layered verdict, override action.
- Schema extension via typed `BTreeMap<String, String>` sidecar; future fields don't change the port.
- Storage stays NDJSON; #38 swaps SQLite against same port.
- Domain free of `serde_json` (closes ADR-0001 drift from `record_custom`).

## Non-goals

- SQLite (#38).
- New CLI filter flags.
- Streaming consumers, log rotation, DB compaction.
- `root_id` persistence (main's query-time parent-chain reconstruction stays).

## Decisions

| Decision | Choice | Notes |
|---|---|---|
| Port shape | `write(record) + write_event(event)` | Replaces 6-method `record_incoming/outcome/error/event/hook_event/custom`. |
| Trait name | `ClassificationHistoryWriter` (kept) | Symmetric with existing `ClassificationHistoryReader`. |
| `ClassificationRecord` | Replaced with rich shape (~25 fields) | Domain artefact; both writer port and reader port use it. |
| Extension sidecar | `details: BTreeMap<String, String>` | Adapter serialises to JSON. Closes ADR-0001 drift (no `serde_json` in domain). Rule: anything that would need nesting becomes a typed top-level field on `ClassificationRecord` / `ClassificationEvent`, not a JSON object stuffed into `details`. |
| Events | `ClassificationEvent { kind, message, correlation_id, details, … }` | `EventKind { Lifecycle, StateTransition }` discriminator. Replaces `record_hook_event` + `record_event` + `record_custom`. |
| Correlation | `CorrelationId(Uuid)` minted at hook entry | Joins events to classifications row. Independent of `CallId` (which is `Option`). |
| `Classifier::classify` | Returns `ClassifierOutcome` | `verdict + raw_verdict + raw_reason + Option<LlmCallStats>`. |
| `LlmClassifier::classify` | Returns `ClassifierOutcome` | Anthropic adapter measures latency + reads `usage`. |
| Layered verdict | `initial_verdict` / `verdict` / `enforced` + `override_action` | Domain function `resolve_enforcement(verdict, dev_bypass) -> (Enforced, Option<OverrideAction>)`. |
| `process_action` | Returns `ClassificationOutcome` | Slice of fields populated by the prompt loop. Composition root merges with identity / pid / duration / dev_bypass-applied fields. |
| `Agent` shape | `enum Agent { Root, Subagent }` | Type-level subagent-ness. `has_parent` becomes `matches!`. Constructors unchanged. |
| `Scope` cap | 8 KiB via newtype validator | Defence-in-depth. Boundary rejects (fail-closed Red per ADR-0002); never silent truncate. |
| `markers` | Removed | Replaced by `dev_bypass` column on the record. |
| `tool_input` capture | Always captured (pre-MVP forensic default) | Log excessively early; revisit before public shipping. CLAUDE.md security policy permits operational data in the 0600 audit log. |
| Best-effort vs fail-closed | Best-effort (unchanged) | Writes still swallow errors per ADR-0002. SQLite (#38) will revisit. |
| NDJSON line format | One JSON object per line, all record fields | No more multiple-event-types-per-call. |

## Domain types

```rust
pub enum Agent {
    Root { id: AgentId },
    Subagent { id: AgentId, parent_id: AgentId },
}

pub struct ClassificationInput {
    pub agent: Agent,
    pub action: Action,
    pub working_dir: PathBuf,
    pub correlation_id: CorrelationId,
}

pub struct CorrelationId(uuid::Uuid);

pub struct ClassifierOutcome {
    pub verdict: Verdict,
    pub raw_verdict: RawVerdict,         // 'green' | 'red' as returned by the model
    pub raw_reason: Option<String>,
    pub llm: Option<LlmCallStats>,       // None for non-LLM classifiers
}

pub struct LlmCallStats {
    pub model: String,
    pub latency: Duration,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

pub enum RawVerdict { Green, Red }
pub enum ScopeOrigin { PreExisting, ClaimedFromPending, Fresh, Edited }
pub enum Enforced { Permit, Deny }
pub enum OverrideAction { UserAllow, UserDeny, UpdateScope, DisableSession, DevBypass }
pub enum EventKind { Lifecycle, StateTransition }

pub fn resolve_enforcement(
    verdict: &Verdict,
    dev_bypass: bool,
) -> (Enforced, Option<OverrideAction>);

pub struct ClassificationRecord {
    pub ts: time::OffsetDateTime,
    // identity
    pub session_id: AgentId,
    pub parent_id: Option<AgentId>,
    pub agent_type: Option<AgentType>,
    pub working_dir: PathBuf,
    pub call_id: Option<CallId>,
    pub correlation_id: CorrelationId,
    pub pid: u32,
    pub hook_duration: Duration,
    // action
    pub tool_name: ActionKind,
    pub tool_input: Option<String>,        // post-narration-stripping; always populated pre-MVP (revisit before public shipping)
    pub bundled_scope_present: bool,
    pub narration_fields_stripped: u32,
    pub args_size_pre: u32,
    pub args_size_post: u32,
    // scope
    pub scope: Option<Scope>,
    pub scope_origin: Option<ScopeOrigin>,
    // standing guidance
    pub project_guidance_path: Option<PathBuf>,
    pub agent_template_path: Option<PathBuf>,
    // LLM call (Some only when classifier ran)
    pub llm_outcome: Option<ClassifierOutcome>,
    // prompt interaction
    pub prompt_shown: bool,
    pub prompt_latency: Option<Duration>,
    pub lock_wait: Option<Duration>,
    // verdict (layered)
    pub initial_verdict: Option<Verdict>,
    pub verdict: Verdict,
    pub enforced: Enforced,
    pub override_action: Option<OverrideAction>,
    // error & dev-bypass
    pub error_chain: Option<String>,
    pub dev_bypass: bool,
    // extension
    pub details: BTreeMap<String, String>,
}

pub struct ClassificationEvent {
    pub ts: time::OffsetDateTime,
    pub session_id: Option<AgentId>,
    pub correlation_id: Option<CorrelationId>,
    pub kind: EventKind,
    pub message: String,
    pub details: BTreeMap<String, String>,
    pub dev_bypass: bool,
}

pub trait ClassificationHistoryWriter {
    fn write(&self, record: ClassificationRecord) -> Result<(), HistoryError>;
    fn write_event(&self, event: ClassificationEvent) -> Result<(), HistoryError>;
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum HistoryError {
    #[error("classification history unavailable")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

Methods return `Result<(), HistoryError>` now. NDJSON adapter still swallows IO errors internally and returns `Ok(())` (ADR-0002 best-effort logging carve-out preserved). Composition root ignores the `Err` arm in this slice. #38 will tighten the SQLite adapter to fail-closed by propagating `Err`; the port shape doesn't change at that point — only the adapter behaviour and one branch in main.

## Reader port

`ClassificationHistoryReader::query` returns `Vec<ClassificationRecord>` (new shape). Existing CLI filters (`agent_id`, `root_agent_id`, `verdict`, `since`, `action`, `limit`) keep working; new fields additive. Parent map built from `parent_id` (unchanged semantics).

`HistoryQuery` unchanged; new filter fields are a follow-up.

## `process_action` shape

Returns `ClassificationOutcome` (subset of `ClassificationRecord` fields the prompt flow populates):

```rust
pub struct ClassificationOutcome {
    pub scope: Option<Scope>,
    pub scope_origin: Option<ScopeOrigin>,
    pub project_guidance_path: Option<PathBuf>,
    pub agent_template_path: Option<PathBuf>,
    pub llm_outcome: Option<ClassifierOutcome>,
    pub prompt_shown: bool,
    pub prompt_latency: Option<Duration>,
    pub lock_wait: Option<Duration>,
    pub initial_verdict: Option<Verdict>,
    pub verdict: Verdict,
    pub override_action: Option<OverrideAction>,
    pub error_chain: Option<String>,
}
```

State transitions inside the loop emit `EventKind::StateTransition` events eagerly via `write_event` so they survive a crashed classification. The final `write(record)` happens in the composition root after merging.

## Composition root

`crates/claude-code-hook/src/main.rs`:

1. Mint `correlation_id = CorrelationId::new()` at entry. Capture `hook_started = Instant::now()`.
2. Open `FileClassificationHistoryWriter::new(log_path)` (no markers, no debug_mode parameter).
3. Build identity slice of the record (`session_id`, `parent_id` via `Agent::Subagent`, `agent_type`, `working_dir`, `call_id`, `correlation_id`, `pid`, action sizes, bundled_scope_present, narration_fields_stripped).
4. Emit lifecycle events via `write_event(EventKind::Lifecycle)` for: `hook-event`, `stage-pending` outcomes, `claim-pending` outcomes, `invalid-agent-type`.
5. Call `process_action(&input, ...) -> ClassificationOutcome`. Merge into `ClassificationRecord`.
6. `let (enforced, dev_override) = resolve_enforcement(&record.verdict, cfg.dev_bypass);`
7. Set `record.enforced`, `record.override_action` (if `dev_override`), `record.hook_duration`, `record.dev_bypass`.
8. `history.write(record)`.
9. Translate `enforced` to exit code: `Permit` → 0; `Deny` → 2 + stderr reason.

`cfg.audit_markers()` is deleted (sole caller gone). `dev_bypass` lives on the record column.

## NDJSON line shape (new)

```json
{
  "ts": "2026-05-21T10:00:00.000Z",
  "session_id": "abc",
  "parent_id": "root",
  "agent_type": "Plan",
  "working_dir": "/Users/x/dev/foo",
  "call_id": "tu-1",
  "correlation_id": "550e8400-…",
  "pid": 1234,
  "hook_duration_ms": 412,
  "tool_name": "Bash",
  "tool_input": "{\"command\":\"ls\"}",
  "bundled_scope_present": false,
  "narration_fields_stripped": 1,
  "args_size_pre": 87,
  "args_size_post": 32,
  "scope": "do the thing",
  "scope_origin": "pre-existing",
  "project_guidance_path": "/Users/x/dev/foo/REMIT.md",
  "agent_template_path": null,
  "llm": {"model": "claude-haiku-4-5", "latency_ms": 312, "input_tokens": 234, "output_tokens": 12},
  "raw_verdict": "green",
  "raw_reason": null,
  "prompt_shown": false,
  "prompt_latency_ms": null,
  "lock_wait_ms": null,
  "initial_verdict": "Green",
  "initial_red_kind": null,
  "initial_reason": null,
  "verdict": "Green",
  "red_kind": null,
  "reason": null,
  "enforced": "permit",
  "override_action": null,
  "error_chain": null,
  "dev_bypass": false,
  "details": {}
}
```

One line per `process_action` call. Events on sibling lines with `event_kind` discriminator: `"classification"` for records, `"lifecycle"` / `"state-transition"` for events. Reader filters by `event_kind`.

Alternative considered: two files (`remit.log` + `remit.events.log`). Rejected: single file matches today's behaviour and keeps multi-process locking simple.

## File-classification-history rewrite

`crates/file-classification-history/src/lib.rs`:

- `FileClassificationHistoryWriter::new(path)` — markers + debug_mode parameters removed.
- `impl ClassificationHistoryWriter`:
  - `write(record)` serialises the whole record as one JSON line, prefixed with `"event_kind":"classification"`.
  - `write_event(event)` serialises as one line with `"event_kind":"lifecycle"` or `"event_kind":"state-transition"`.
- Same flock + single-syscall write pattern as today (TN-2026-006).
- `FileClassificationHistoryReader::query` parses each line; dispatches on `event_kind`; materialises records from `"classification"` lines; ignores event lines for query purposes.
- Parent map built from `parent_id` field on classification lines.

## Adapter touchpoints

- `anthropic-classifier`: return `ClassifierOutcome` with `LlmCallStats`. Measure latency around HTTP call. Read `usage.input_tokens` / `usage.output_tokens`.
- `file-scope-store`: `Agent` enum change ripples into `scope_for(&agent.id())` callers; otherwise unchanged. No new methods.
- `remit-cli`: adapt to new `ClassificationRecord` fields. CLI output may surface new fields; scope is light.

## Domain `flow.rs` rewrite

`process_action` returns `ClassificationOutcome`, built branch-by-branch:

- After `resolve_scope`: set `scope`, `scope_origin = PreExisting` (when found).
- After classifier call: set `llm_outcome`, set `initial_verdict` (first call only).
- `user-allow`: `override_action = UserAllow`, `verdict = Green`.
- `user-deny`: `override_action = UserDeny`.
- `update-scope`: `override_action = UpdateScope`; loop continues; subsequent `verdict` replaces.
- `disable-session`: `override_action = DisableSession`, `verdict = Green`.
- Port error: `error_chain` populated.

`flow::root_key` stays (disabled-flag root-key lookup). No `root_id` persistence in this slice.

`Agent::has_parent` callers switch to `matches!(agent, Agent::Subagent { .. })`.

## Test plan

TDD. Each chunk drives its own failing tests first.

### Domain unit / integration

- `Agent` enum: constructors, `id()`, `parent_id()`; `Agent::subagent(id, id)` collapses to `Root`.
- `Scope::try_new` accepts ≤8 KiB, rejects >8 KiB.
- `resolve_enforcement` covers all four (verdict × dev_bypass) cells.
- `ClassifierOutcome` happy + error paths.
- `process_action` returns a `ClassificationOutcome` whose layered verdict / override_action match the expected branch (Green; Red → user-allow → Green; Red → user-deny; Red → update-scope → Green; Red → disable → Green; subagent Red with no prompt).
- Standing guidance error path populates `error_chain`.
- Event log captures expected state transitions in order.

### File-classification-history

- `write(record)` produces a valid JSON line with all populated fields; missing optional fields omitted (`skip_serializing_if = "Option::is_none"`).
- `write_event` produces a `lifecycle` / `state-transition` line.
- Concurrent writers across processes: N=16 × M=50, no corruption, all rows present (mirrors TN-2026-006).
- Reader skips event lines; query against records only; parent map built from classification lines.

### Composition root

- End-to-end against a tempfile NDJSON, driving a Green and a Red path.
- Layered verdict + override assertions:
  - Red → user-allow: row shows `initial_verdict=Red, verdict=Green, enforced=Permit, override_action=UserAllow`.
  - Red → dev_bypass: `initial_verdict=Red, verdict=Red, enforced=Permit, override_action=DevBypass`.
- Lifecycle events captured: `hook-event`, `invalid-agent-type` (when applicable).

### Anthropic-classifier

- Existing tests adapted to assert `ClassifierOutcome` shape (raw verdict, raw reason on Red, LlmCallStats populated).
- Latency is measured but not asserted on exact value (smoke: `> Duration::ZERO`).
- `usage` parsed from response; missing usage → `input_tokens`/`output_tokens` = `None`.

### remit-cli

- Existing `remit logs` filters still work with new record shape.
- CLI output: any new flag (e.g. `--show-llm`) is a separate issue.

## Dependencies

- Domain: add `time` (already in main? confirm), `uuid = { version = "1", features = ["v4"] }`. Drop `serde_json` from domain (closes ADR-0001 drift).
- file-classification-history: keep `serde` / `serde_json` / `time`.
- anthropic-classifier: no new deps; uses `Instant::now`.

## Touched files

- `crates/domain/Cargo.toml` — add `uuid`; remove `serde_json` (was only used for `record_custom`).
- `crates/domain/src/lib.rs` — Agent enum, ClassificationInput.correlation_id, ClassifierOutcome + LlmCallStats, ScopeOrigin, Enforced, OverrideAction, RawVerdict, CorrelationId, EventKind, ClassificationEvent, ClassificationOutcome (re-exported), ClassificationRecord (rewritten), ClassificationHistoryWriter (new shape: write + write_event), resolve_enforcement, Scope 8 KiB cap. Drop record_hook_event / record_custom from trait.
- `crates/domain/src/flow.rs` — rewrite `process_action` to return `ClassificationOutcome`; emit events eagerly via `write_event`; drop `record_incoming` / `record_outcome` / `record_error` calls.
- `crates/domain/src/test_support.rs` — `agent_store_contract` unchanged (`AgentStore` shape unchanged). `InMemoryClassificationHistory` reader contract: adapt to new `ClassificationRecord`.
- `crates/domain/tests/common/mod.rs` — `RecordingHistory` test fake (replaces `RecordingLogger`). FakeLlmClassifier returns `ClassifierOutcome`.
- `crates/domain/tests/*.rs` — call-site updates; new layered-verdict / override-action assertions.
- `crates/anthropic-classifier/src/lib.rs` — return `ClassifierOutcome`; measure latency; extract usage.
- `crates/anthropic-classifier/src/tests.rs` — assertions on new outcome shape.
- `crates/file-classification-history/src/lib.rs` — new port impl (write + write_event). Drop markers parameter. Single-line-per-record JSON shape.
- `crates/file-classification-history/src/tests.rs` — rewrite for new line shape.
- `crates/remit-cli/src/main.rs` — adapt to new `ClassificationRecord` fields if any output references the old fields; verify filter behaviour unchanged.
- `crates/claude-code-hook/src/main.rs` — wire correlation_id, mint identity slice, call `process_action`, merge ClassificationOutcome, call `resolve_enforcement`, `history.write(record)`. Replace lifecycle `record_hook_event` / `record_custom` calls with `write_event`. Remove `cfg.audit_markers()` reference.
- `crates/claude-code-hook/src/tests.rs` — fixtures for new record shape.
- `crates/remit-config/src/lib.rs` — delete `audit_markers()`.
- `crates/remit-config/src/tests.rs` — drop the marker tests.
- `docs/CONFIGURATION.md` — drop the `[dev-bypass-active]` marker mention; reference dev_bypass column.
- `docs/decisions/adr/ADR-0001-hexagonal-architecture.md` — port list refresh (new shape; no `serde_json` in domain).
- `docs/tn/README.md` — index entry.

## Rollout

Single trunk commit (large but reviewable as discrete sections). Wipe `~/.remit/remit.log` before running locally — old NDJSON shape won't parse against the new reader.

Smoke:
1. Drive a Green: one line, `verdict=Green`, `enforced=permit`, `llm.model` populated.
2. Drive a Red without prompt (subagent): `prompt_shown=false`, `enforced=deny`.
3. Drive a Red → user-allow: layered verdict captured.
4. Drive a Red with `dev_bypass=true`: `enforced=permit`, `override_action=dev-bypass`.
5. `remit logs --verdict red` returns the expected rows.

## Open questions

- `LlmCallStats` tokens: `Option<u32>` (lean) vs `u32` with `0` for missing. `0` is misleading.
- Anthropic adapter retry: single-attempt today. Out of scope; flag follow-up.
- CLI flag additions for new fields: out of scope; flag follow-up.

## Follow-ups (architect review)

- When #38 swaps in SQLite, the on-disk shape diverges from the in-memory `ClassificationRecord`. At that point split into a write-side `ClassificationRecord` and a read-side `ClassificationHistoryEntry` (don't pre-split now).
