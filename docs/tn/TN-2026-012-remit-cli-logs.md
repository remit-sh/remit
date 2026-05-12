---
number: 2026-012
title: remit CLI with `remit logs` + NDJSON migration
kind: plan
status: active
issue: "#39"
tags: [cli, observability, ndjson, query-port, file-classification-log]
---

# TN-2026-012: `remit` CLI with `remit logs`

> `remit` binary queries every classification post-hoc via a domain port. Filters: agent, root, verdict, since, action. Foundation for the SQLite migration in #38.

## Problem

- No way to inspect classifications after the fact. Only audit surface is `~/.remit/remit.log`, free-text, grep-only.
- Hook lifecycle, classification, and ad-hoc events interleaved in one stream.
- `FileLogger` docstring claims NDJSON but emits prose lines (`incoming: id=sess kind=Bash args=ls`). Drift flagged in TN-2026-006.
- Action args logged via `args=...`. CLAUDE.md says they shouldn't — pre-existing security drift.

## Outcome

- `remit logs [--agent ID] [--root ID] [--verdict green|red] [--since 10m] [--action Bash] [--limit N] [--show-args]` returns matching classifications.
- Domain owns a single query port; adapters interchangeable (NDJSON file today, SQLite tomorrow per #38).
- Logger output is real NDJSON.
- `debug_mode = true` in config turns on full-payload capture (`args` on `incoming` records). Off by default — keeps the durable record narrow per CLAUDE.md security policy.

## Approach

Five landings, each shippable on its own:

1. **Crate rename.** `file-logger` → `file-classification-log` (writer + future reader are one role). Pure rename.
2. **NDJSON write path.** Writer emits structured records. Closes TN-006 drift. Drops `args=` (CLAUDE.md security drift).
3. **Query port.** `ClassificationLog` trait + types in domain. In-memory fake under `test_support`.
4. **Read adapter.** `FileClassificationLog` in `crates/file-classification-log/`.
5. **`remit` binary.** New `crates/remit-cli/` with `clap` subcommands; `remit logs` first.

Landings 3–5 can interleave once 1+2 land. Landing 2 splits internally: 2a structured records for the `ClassifierLogger` impl methods, 2b typed helpers replace `log_line` callsites in `claude-code-hook` (`log_hook_event`, `log_stage_pending`, …). 2a unblocks the reader; 2b can land separately without blocking.

## Design

### Domain port

In `crates/domain/src/lib.rs`:

```rust
#[derive(Debug, Default)]
pub struct LogQuery {
    pub agent: Option<AgentId>,
    pub root: Option<AgentId>,
    pub verdict: Option<VerdictFilter>,
    pub since: Option<SystemTime>,
    pub action: Option<ActionKind>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerdictFilter { Green, Red }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassificationRecord {
    pub timestamp: SystemTime,
    pub agent_id: AgentId,
    pub parent_id: Option<AgentId>,
    pub action: ActionKind,
    pub verdict: Verdict,
    pub working_dir: PathBuf,
    /// Opaque, adapter-stringified action args. `Some` only when the record
    /// was captured under `debug_mode`. Domain treats this as data — no
    /// parsing, no semantic interpretation.
    pub args: Option<ActionArgs>,
}

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum LogQueryError {
    #[error("classification log unavailable")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait ClassificationLog {
    fn query(&self, q: &LogQuery) -> Result<Vec<ClassificationRecord>, LogQueryError>;
}
```

- `SystemTime` and `PathBuf` are std; allowed in domain.
- `Verdict` already carries Green/Red + `RedKind` + `reason`. No duplication.
- No `args` field by default. Action args stay out of the durable record (closes CLAUDE.md drift); see Debug mode for opt-in capture.
- Single `Unavailable` error variant. Missing log is **not** an error — adapter returns `Ok(vec![])`. Read-side fail-closed (matching exit codes etc.) belongs to the caller; the port describes "can this log be read?", not "should the action proceed?".
- Returned `Vec` is fine for v1. Streaming iterator deferred until log volume needs it.
- NDJSON serde structs and time-crate types live in the adapter crate, never in domain.

### Root resolution

`--root R` returns rows whose `agent_id` is in descendants(R), inclusive. Descendant set built from the records: the adapter parses **both `incoming` and `outcome`** records to construct a child→parent map (covers the case where a parent agent spawns a child before issuing any actions of its own), then filters `outcome` records by membership. No new persistent state.

### NDJSON record shape

One JSON object per line. `event` field discriminates:

```jsonc
// classification (one per process_action call)
{ "ts": "2026-05-12T10:14:33.421Z", "event": "incoming", "agent_id": "...", "parent_id": "...", "call_id": "...", "action": "Bash", "working_dir": "/path" }
{ "ts": "2026-05-12T10:14:34.118Z", "event": "outcome",  "agent_id": "...", "parent_id": "...", "action": "Bash",  "working_dir": "/path", "verdict": "green" }
{ "ts": "...", "event": "outcome", "agent_id": "...", "parent_id": "...", "action": "Write", "working_dir": "/path", "verdict": "red", "red_kind": "OutOfScope", "reason": "writes outside scope" }
{ "ts": "...", "event": "error",   "agent_id": "...", "chain": "outer: inner: disk full" }

// hook lifecycle (composition root)
{ "ts": "...", "event": "hook",           "name": "PreToolUse" }
{ "ts": "...", "event": "stage_pending",  "parent": "...", "outcome": "set" }
{ "ts": "...", "event": "claim_pending",  "hook": "SubagentStart", "parent": "...", "child": "...", "outcome": "hit" }

// with debug_mode = true, incoming AND outcome records carry args:
{ "ts": "...", "event": "incoming", "agent_id": "...", "action": "Bash", "args": "{\"command\":\"ls -la ~/.ssh\"}", "markers": ["debug-mode"] }
{ "ts": "...", "event": "outcome",  "agent_id": "...", "action": "Bash", "args": "{\"command\":\"ls -la ~/.ssh\"}", "verdict": "red", "red_kind": "OutOfScope", "reason": "...", "markers": ["debug-mode"] }
```

- `ts` is RFC 3339 UTC, ms precision, always first key.
- Unknown `event` types tolerated by readers; `remit logs` filters to `outcome`.
- Audit markers (`dev-bypass-active`, `debug-mode`) move from line prefix to a `markers: [...]` field on every record.
- `outcome` records carry every field needed for a `ClassificationRecord` (agent_id, parent_id, action, working_dir, verdict). Reader walks outcomes only; incoming + error + hook lines are decorative for the CLI. Cost: wire duplication. Benefit: one-line-per-record scan, no pairing.

### Debug mode

New config field (`remit-config`):

```toml
debug_mode = true   # default: false
```

- When **off** (default): records carry no action args. Matches CLAUDE.md ("Sensitive content … never logged").
- When **on**: both `incoming` and `outcome` records carry the full (narration-stripped) `args` JSON, stringified. Outcome too — so `remit logs --show-args` works from a one-pass outcome scan. Logger receives the flag at construction (`FileLogger::new(..., debug_mode)`) and the `ClassifierLogger` impl emits or omits the field accordingly.
- `"debug-mode"` appears in `markers` on every record captured under debug mode — audit trail shows when verbose capture was active.
- Composition root logs a one-line stderr warning at startup when `debug_mode = true`, mirroring the `dev_bypass` pattern. Forensic transparency: the user is reminded that sensitive content is now being persisted.
- Picked up by `RemitConfig::audit_markers()` so the marker is computed once and applied consistently.

### Read adapter

`crates/file-classification-log/` (same crate as writer; format owner colocated):

```rust
pub struct FileClassificationLog { path: PathBuf }

impl ClassificationLog for FileClassificationLog { ... }
```

- Opens log read-only with a shared lock (cooperates with writer's exclusive lock).
- Streams line by line, parses each as JSON.
- Two passes: pass 1 builds the parent map from `incoming` + `outcome`; pass 2 materialises `ClassificationRecord`s from `outcome` lines, applying filters in memory.
- **Missing file** → `Ok(vec![])`. Fresh-install case dominates; the port answers "can the log be read", not "should the action proceed".
- **Malformed line** → `LogQueryError::Unavailable`. Security tool argues against silent skips; misleading audit output is worse than a clear error.
- **Order**: records returned in file (append) order, which is approximately chronological under the writer's exclusive-lock contract.

### `remit` binary

New crate `crates/remit-cli/`:

```
crates/remit-cli/
├── Cargo.toml
├── src/
│   ├── main.rs           clap parse + dispatch
│   ├── logs.rs           `logs` subcommand: build LogQuery, render rows
│   ├── since.rs          `-10m` / `-1h` / `-30s` parser
│   └── tests.rs
└── tests/integration.rs  end-to-end against fixture NDJSON
```

Dependencies: `domain`, `file-classification-log`, `remit-config`, `clap` v4 (derive), `humantime` (or hand-roll).

Composition pattern matches `claude-code-hook`: `main.rs` wires `RemitConfig::log_path()` → `FileClassificationLog` → query → render.

Output (v1): tab-separated columns:

```
TS                     AGENT       ACTION     VERDICT   REASON
2026-05-12T10:14:34Z   agent-abc   Bash       green
2026-05-12T10:14:35Z   agent-abc   Write      red       writes outside scope
```

`--json` deferred.

### CLI flags

| Flag | Type | Notes |
|---|---|---|
| `--agent <ID>` | string | Parsed as `AgentId` |
| `--root <ID>` | string | Parsed as `AgentId` |
| `--verdict <green\|red>` | enum | clap value-parser |
| `--since <DUR>` | duration | `10m`, `1h30m`, `30s` |
| `--action <NAME>` | string | Parsed as `ActionKind` |
| `--limit <N>` | usize | Default 100; `0` = unlimited. Returns the **most recent** N records. |
| `--show-args` | flag | Display `args` (or note when absent) for matching records. Off by default. |

- `--since` is relative to `now()`. Absolute timestamps deferred.
- `--agent` is the vocab-correct flag; `--session` would re-introduce a Claude-Code-specific term that the domain explicitly forbids.
- `--show-args` only surfaces what's already captured. Records logged without `debug_mode` have no `args` and the column reads `-`.

## Sequencing

- **Depends on / subsumes #41** (timestamps in `remit.log`). NDJSON migration here adds `ts` to every record. Whichever lands first, the other rebases; coordinate with the #41 worktree before merging.
- **Parallel with #38** (SQLite). Both target the same domain port. SQLite-backed adapter is a second `ClassificationLog` impl; the CLI is unchanged.

## Migration plan

**Landing 1 — crate rename**
1. `git mv crates/file-logger crates/file-classification-log`; rename `Cargo.toml` package and workspace member; update `claude-code-hook/Cargo.toml`.
2. No code changes. Pre-push hook green.

**Landing 2a — NDJSON write path (`ClassifierLogger` impl)**
3. Add `serde`, `serde_json`, `time` (RFC 3339 formatting + UTC) to `crates/file-classification-log/Cargo.toml`. Time crate is `time`, not `chrono`.
4. Add `debug_mode: bool` to `RemitConfig` (default `false`). `audit_markers()` appends `"debug-mode"` when set.
5. Replace string `format!` in `FileLogger`'s `ClassifierLogger` impl with NDJSON-emitting helpers. Constructor takes a `debug_mode` flag. `outcome` records carry every `ClassificationRecord` field; `args` field present only when `debug_mode`.
6. Composition root: stderr warning at startup if `debug_mode = true`, mirroring `dev_bypass`.
7. Audit markers become a per-record `markers: [...]` field.
8. Update `file-classification-log/tests/integration.rs` to assert NDJSON shape, debug-mode args inclusion, and that default-mode records carry no `args`; concurrent-writes test (TN-006) still passes.

**Landing 2b — typed helpers replace `log_line`**
9. Add `log_hook_event`, `log_stage_pending`, `log_claim_pending` (etc.) on `FileLogger`. Each emits an NDJSON record with the right `event` discriminator.
10. Update `claude-code-hook/src/main.rs` to call them; remove `log_line` from the public API.
11. Default-mode records never carry action args. Closes CLAUDE.md security drift.

**Landing 3 — query port (domain)**
12. Add `ClassificationLog`, `LogQuery`, `ClassificationRecord` (with `args: Option<ActionArgs>`), `VerdictFilter`, `LogQueryError` to `crates/domain/src/lib.rs`.
13. In-memory fake under `domain::test_support` (gated on the existing `test-support` feature).
14. Filter combination tests against the fake.

**Landing 4 — read adapter**
15. `FileClassificationLog` in `crates/file-classification-log/`. Two-pass parser (parent map + record materialisation).
16. Integration tests with fixture NDJSON files (tempdir), including debug-mode records.

**Landing 5 — `remit` CLI**
17. Scaffold `crates/remit-cli/` binary.
18. `remit logs` wiring + rendering, with `--show-args` rendering the `args` field when present.
19. End-to-end test: fixture log, run binary, assert stdout (both with and without `--show-args`).
20. Workspace `Cargo.toml`: add `remit-cli` member.

Each landing is a separate commit. 2b may follow 2a at any cadence; 3 only depends on the domain itself; 4 depends on 2a + 3; 5 depends on 3 + 4.

## Test strategy

TDD throughout.

### Domain port (`crates/domain/tests/log_query.rs`)
- Empty query returns all.
- Each filter narrows correctly: agent, root (descendant walk), verdict, since, action, limit.
- Filters compose (verdict=red AND action=Write).
- Limit applied after filtering.
- `since` boundary: strictly ≥.

### Read adapter (`crates/file-classification-log/tests/query.rs`)
- Mixed `event` types in fixture: only `outcome` rows materialise.
- Unknown event types ignored.
- Malformed JSON line → `Unavailable`.
- Missing file → `Ok(vec![])`.
- Records returned in file order; assert chronological under monotonic timestamps.
- Reader's shared lock vs writer's exclusive lock: behaviour documented + asserted.

### NDJSON writer (extends `crates/file-classification-log/tests/integration.rs`)
- Every line is valid JSON.
- `ts` always present, RFC 3339, UTC.
- `outcome` records carry all `ClassificationRecord` fields.
- Markers appear as a field, never as a prefix.
- Default mode: `args` absent from every record.
- `debug_mode = true`: `args` present on both `incoming` and `outcome`; `"debug-mode"` in `markers`.
- TN-006 concurrent-writers test still passes.

### CLI (`crates/remit-cli/tests/integration.rs`)
- `assert_cmd`: fixture log under tempdir, `REMIT_HOME` set, invoke `remit logs --verdict red`, parse stdout.
- `--since 10m` filters older records (advance fixture timestamps).
- `--show-args` renders args when present, `-` otherwise.
- Bad input (`--verdict purple`): non-zero exit, friendly stderr.
- Missing log file: exits 0 with empty result.

## Open questions

1. **`args` field downstream.** Audit consumers of the existing `args=...` line content (logs, scripts, ops dashboards) before Landing 2b drops it from default-mode records. None expected; verify.
2. **`debug_mode` default.** False — sensitive content shouldn't be persistent by default. Confirm.

## Out of scope

- SQLite adapter (#38, independent).
- `--json` output format.
- Other subcommands (`remit scope`, `remit allow`, …).
- Streaming iterator from the port.
- Persistent root_id at write time (saves the descendant walk; defer until query latency hurts).
- `--follow` / tail mode.
- Log rotation, size caps.

## Files touched

**New**
- `docs/tn/TN-2026-012-remit-cli-logs.md`
- `crates/remit-cli/Cargo.toml`
- `crates/remit-cli/src/{main,logs,since,tests}.rs`
- `crates/remit-cli/tests/integration.rs`
- `crates/domain/tests/log_query.rs`
- `crates/file-classification-log/tests/query.rs`

**Rename**
- `crates/file-logger/` → `crates/file-classification-log/` (Landing 1)

**Modify**
- `Cargo.toml` (workspace: add `remit-cli`, rename `file-logger` member)
- `crates/domain/src/lib.rs` (port + types)
- `crates/remit-config/src/lib.rs` (add `debug_mode: bool`; `audit_markers()` appends `"debug-mode"`)
- `crates/remit-config/src/tests.rs` (debug-mode load + marker tests)
- `crates/file-classification-log/src/lib.rs` (NDJSON writer; `FileClassificationLog` reader)
- `crates/file-classification-log/Cargo.toml` (`serde`, `serde_json`, `time`)
- `crates/file-classification-log/tests/integration.rs` (NDJSON shape, debug-mode args)
- `crates/claude-code-hook/src/main.rs` (typed helpers replace `log_line`; debug-mode flag wired into logger)
- `crates/claude-code-hook/Cargo.toml` (rename `file-logger` dep)
- `docs/tn/README.md` (index entry)

## Risks

- **NDJSON migration breaks downstream tooling.** Mitigation: audit known consumers before Landing 2b (which removes the free-text shape).
- **#41 conflict.** Same file, same goal. Mitigation: coordinate worktrees; first one in wins, other rebases.
- **Crate rename ripples to external references.** Mitigation: Landing 1 is a pure rename, no behaviour change — easy revert if needed.
