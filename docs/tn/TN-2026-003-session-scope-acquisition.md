---
number: 2026-003
title: Agent scope acquisition
kind: plan
status: active
issue: "#17"
tags: [scope, prompter, hook, mvp, bundled-scope]
---

# TN-2026-003: Agent scope acquisition

> How every **Agent** — root, in-process subagent, peer-process subagent — comes to have its own **Scope**. Implements #17. Replaces the earlier draft of this TN, which proposed parent-walk inheritance.

## Goal

- Every tool call has a Scope to classify against.
- Each Agent has its **own** Scope. No inheritance from parent.
- Two acquisition paths, picked by Agent topology:
  - **Root**: user types it via a system dialog at first tool call.
  - **Subagent** (in-process or peer-process): lifted automatically from the parent's spawn-tool payload (`Agent.prompt`) at spawn time. **Bundled scope.**

## Why bundled, not inherited

- Subagent inheritance silently runs a child under the parent's broader Scope. Wrong direction — children should be narrower or in some cases different, not equal.
- The parent's spawn intent is already in the spawn-tool args; remit lifts it without touching the agent. **Invisible to agents** — no remit-aware behaviour required.
- Hook payloads make this safe: the Scope is captured at the parent's `PreToolUse` (which the child can't influence) and bound to the child's identity at its start hook.

## Model

```
parent agent's PreToolUse(Agent)                      ──► hook stages scope under parent_id
                                                          (PendingScopeStore::set)
SubagentStart  / SessionStart   (parent_id, child_id) ──► hook claims under parent_id,
                                                          assigns under child_id
                                                          (PendingScopeStore::take + ScopeWriter::assign)
child's PreToolUse(any tool)                          ──► classifier reads scope by child_id;
                                                          present → Green; absent → Red
```

- Root flow unchanged from the spike: hook detects no scope on a parent-less Agent at first PreToolUse → `ScopePrompter::prompt` (osascript dialog) → `ScopeWriter::assign` → Green.
- `resolve_scope` is **direct lookup**. No parent walk.

## Domain ports

- `ScopeReader::scope_for(agent_id) → Option<Scope>` — direct read.
- `ScopeWriter::assign(agent_id, scope)` — bind a Scope to an Agent.
- `PendingScopeStore::set(parent_id, scope)` / `take(parent_id) → Option<Scope>` — single-slot per parent; `take` consumes.
- `ScopePrompter::prompt(agent_id) → Scope` — root-only; `Cancelled` and `Unavailable` are distinct errors.
- `ClassifierLogger::log_incoming(input)` — outbound observability.

## Domain types

- `Agent { id: AgentId, parent_id: Option<AgentId> }`. Constructors: `Agent::root(id)`, `Agent::subagent(id, parent)`. Self-referential parent collapses to root.
- `Scope` — non-empty after trim. Empty/whitespace bundled scope is rejected at the boundary, not staged.
- `RedKind { NoScopeRoot, ScopeSetupCancelled, ScopeSetupUnavailable, SubagentMissingScope, StoreUnavailable }`. `Verdict::Red { kind, reason }`.
- `ScopeStoreError::Unavailable(_)` — the only store-side error variant.

## Adapters

- `FileScopeStore` — `~/.remit/scopes/<agent_id>.json`. Implements `ScopeReader` + `ScopeWriter`.
- `FilePendingScopeStore` — `~/.remit/pending-scopes/<parent_id>.json`. Implements `PendingScopeStore`. `take` deletes after read (best-effort).
- `OsascriptPrompter` — macOS dialog via `osascript`. Distinguishes user-cancel from missing-binary failure.
- `FileLogger` — append-only NDJSON. Adds `log_line(&str)` for hook to record `hook-event`, `stage-pending`, `claim-pending`.
- `claude-code-hook` — composition root. Dispatches by `hook_event_name`:
  - `PreToolUse`: extract bundled scope from `Agent.prompt` (allowlist `BUNDLED_SCOPE_FIELDS`); identity-collapse the in-process subagent's `agent_id` into the `Agent.id`; classify; exit 2 on Red unless `dev_bypass`.
  - `SubagentStart`: claim under payload `session_id`, assign to payload `agent_id`. Exit 2 + stderr if `assign` fails after `take`.
  - `SessionStart`: claim under env `CLAUDE_SESSION_ID` (peer-process subagent), assign to payload `session_id`. Root sessions (no env var) skip — root prompts via PreToolUse.

## Identity collapse

Hook adapter is responsible for translating Claude Code wire types into the domain `Agent`:

| Topology | `Agent.id` | `Agent.parent_id` |
|---|---|---|
| Top-level session | payload `session_id` | `None` (or env `CLAUDE_SESSION_ID` if present and distinct) |
| In-process subagent (`agent_id` set) | payload `agent_id` | payload `session_id` |
| Peer-process subagent | payload `session_id` | env `CLAUDE_SESSION_ID` |

The `Agent` constructors enforce the self-referential-parent collapse. The hook unit-tests `parent_from_env_value` to pin the env-var filter.

## Fail-closed boundaries

- Empty/whitespace bundled scope → not staged. Child hits `SubagentMissingScope` at first tool call.
- Pending-store IO failure on `set` → exit 2 (parent's spawn call denied; no half-state).
- `take` succeeds but `assign` fails → log + exit 2 + stderr (operator signal). Pending row already gone, child runs unscoped if the spawn proceeds; the explicit failure prevents silent drift.
- Malformed agent ids in any payload → `fail_closed("…", AgentId::try_new(...))` → exit 2.
- `Scope` validation failure on stored data → `ScopeStoreError::Unavailable` (corrupt row treated as fail-closed).

## What's tested

- Domain: `Agent::root`/`subagent` constructors and self-collapse; `resolve_scope` direct lookup (no parent walk); `acquire_scope` six paths; `Classifier` Verdict+RedKind matrix; `Scope` non-empty validation; `PendingScopeStore` contract (`set`/`take`/single-shot/independence/sequential cycles/overwrite-without-take/persistence).
- File adapters: `ScopeReader`+`ScopeWriter` contract via `domain::test_support::scope_store_contract`; `PendingScopeStore` contract via `pending_scope_store_contract`; corrupt scope file → `Unavailable`.
- Hook: `bundled_scope_from` allowlist behaviour (Agent only, missing/empty/whitespace/non-string field rejected); `BUNDLED_SCOPE_FIELDS` snapshot pinned; `parent_from_env_value` filters self-equality and invalid ids.
- Live: 10-parallel in-process subagent spawn (10/10 hits, no overwrites). 10-parallel team-spawn (9/10 hits, 1 overwrite — see "Known limits" below).

## Known limits

- **Parallel team spawns**: peer-process spawn lifecycle is async — `SessionStart` fires after the parent has issued the next `Agent`. Single-slot `PendingScopeStore` collides under that load. Tracked in [#20](https://github.com/remit-sh/remit/issues/20). FIFO queue is the planned fix.
- **Stale pending files**: parent stages, child never starts → row persists indefinitely. Acceptable for MVP; cleanup tied to the #20 work.
- **Upstream correlation**: `SubagentStart`/`SessionStart` payloads don't expose the spawning `tool_use_id`. Reliable disambiguation under parallel spawns needs that field upstream.

## Vocabulary

Per [`docs/UBIQUITOUS_LANGUAGE.md`](../UBIQUITOUS_LANGUAGE.md). `Agent` is the domain umbrella; `Session` is Claude-Code-specific runtime jargon, used only when discussing CC internals (the `session_id` field, env var, etc.). The previous `Session`/`SessionId`/`SubagentId` domain types are gone.

## Out of scope (per #17)

- LLM classifier; `remit.md` policy; role templates; multi-runtime adapters.
- SQLite logging.
- Cross-platform prompter (osascript stand-in until a non-macOS host demands it).
- Config-file override for the scope/pending dirs.
- Hook-timeout protection for slow user input on the prompter.
- FIFO pending queue (#20).
