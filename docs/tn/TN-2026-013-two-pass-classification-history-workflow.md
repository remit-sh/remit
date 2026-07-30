---
number: 2026-013
title: Two-pass classification with session history and workflow enforcement
kind: proposal
status: draft
tags: [classifier, llm, history, workflow, vocab]
---

# TN-2026-013: Two-pass classification with session history and workflow enforcement

> Three coupled shifts: a summary pass before the classify pass; bounded session history fed to the classifier; a new **Standing guidance** slot for procedural rules. Together they extend Remit from per-action relevance to multi-action procedure.

## Problem

- Classifier sees one **Action** in isolation. Can't catch "you pushed without running the review step".
- Action args reach the classifier as raw JSON of varying shape. Adapter has to do its own narration-stripping (TN-005 #34) on input the LLM then re-interprets.
- **Scope** is relevance, not procedure (`UBIQUITOUS_LANGUAGE.md`). Rules of the form "X must precede Y" have no home.

## Proposal

### Three coupled changes

1. **Pass 1 — summary.** LLM call producing a short plain-English description of what the action does.
2. **Pass 2 — classify.** Existing call, now taking the summary instead of raw args, plus bounded session history.
3. **Workflow rules.** New **Standing guidance** slot expressing procedural obligations in natural language.

Coupled because pass 2 needs a normalised input to make history-aware reasoning tractable, and workflow rules need history to anchor against.

### Pass 1 — Action summary

- New domain port `ActionSummariser::summarise(action) -> Result<ActionSummary, SummariserError>`.
- `ActionSummary` is a newtype around a bounded string (~200 chars). No tool name leak (already in `Action`), no scope leak (caller doesn't pass it).
- Adapter prompt: imperative, single sentence, drop framing. Reuse the spirit of TN-005 #35's what-extractor.
- Adapter reuses the same Anthropic config; same fail-closed semantics.

### Pass 2 — Classify with history

- `LlmClassifier::classify` signature widens:
  - takes `&ActionSummary` (replacing args inside the prompt — args stay on `Action` for logging only)
  - takes `&SessionHistorySnapshot`
  - takes `&StandingGuidance` (already implicit; make it explicit)
- LLM prompt gains a `<history>` section and a `<workflow_rules>` section. Same data-only framing as `<scope>` / `<action>`.

### Session history

- New domain port `SessionHistory::recent(agent_id, budget) -> Result<SessionHistorySnapshot, HistoryError>`.
- `SessionHistorySnapshot` = ordered list of `{timestamp, action_kind, summary, verdict}` — **no raw args**. Summaries only.
- Bounding: token budget at the adapter, not count. Newest-first, drop oldest.
- Adapter `file-logger-history` reads the existing NDJSON log. Single source of truth — no new persistence (defer SQLite to TN-2026-038 if/when promoted).
- **Requires that pass-1 summaries be logged** — extend the log schema with a `summary` field on classification records (`TN-2026-004`).

### Workflow rules — new Standing guidance slot

- Third slot alongside **Project guidance** (`REMIT.md`) and **Agent template** (`.remit/agent-templates/<type>.md`).
- Authoring surface: a `## Workflow` section inside `REMIT.md` (cheapest format; no new file). Per-agent-type override possible via a section in the agent template.
- Examples (natural language; classifier interprets against history):
  - "A push to main must be preceded by a review step earlier in the session."
  - "Spawning a research subagent requires a prior planning action."
- Deliberately **runtime-agnostic** — no skill names, no command names. Rules describe *what happened*, not *which Claude Code skill ran*.

### Vocabulary

New term needed — **Scope** stays relevance-only. Candidates:

- **Procedure** — neutral; pairs with "procedural rule".
- **Protocol** — overloaded with networking.
- **Workflow obligation** — wordy.
- **Observance** — old-fashioned; clean.

Recommend **Procedure** for the term, with individual rules called **Workflow rules** (informal). Defer ratification to a vocab CDR if this TN is promoted.

## Architectural fit

- **Hex compliance**: two new ports, both in domain. Two new adapters (`action-summariser-anthropic`, `file-logger-history`). Adapter → adapter forbidden, so `file-logger-history` reads the same NDJSON file but doesn't depend on the `file-logger` crate.
- **Fail-closed ladder** extended:
  - Pass-1 `Err` → `Red(ClassifierUnavailable)`. Pass 2 not run.
  - History read `Err` → empty snapshot, classification proceeds. Rationale: a workflow rule requiring evidence will Red against an empty snapshot anyway. A logger outage shouldn't halt the firewall.
  - Pass-2 `Err` → `Red(ClassifierUnavailable)`. Unchanged.
- **Composition root** wires both adapters. Domain stays clean.

## Cost and latency

- **2× LLM calls per tool call.** Currently one Haiku call per `PreToolUse`. This doubles it.
- Pass 1 and Pass 2 are sequential (pass 2 input depends on pass 1 output). History fetch can run parallel to pass 1.
- Mitigations to consider in a follow-up plan, not commit here:
  - Cheaper / smaller model for pass 1.
  - Skip pass 1 for actions whose args are already short and self-evident (e.g. `Read /etc/passwd`).
  - Cache the summary for repeated identical actions in a session.
- **Recommend a measurement spike before promoting this TN to a plan**: instrument one branch with the two-pass shape on a synthetic session of N tool calls; record p50/p95 added latency and provider cost delta.

## Security posture

- **No new disclosure surface in the Anthropic deployment** (TN-005 already routes through Anthropic). Provider sees the args twice (summary + classify) — same data, same 8 KB cap.
- **Pass-1 prompt injection**: summary is downstream input to pass 2. Hostile args could craft a "summary" that recasts the action favourably. Mitigation: pass 2 prompt instructs the model to treat the summary as a hint, not ground truth; raw `action_tool` + truncated args still wrap in `<action_input>` for pass 2's own read.
- **History as injection vector**: a prior Red reason now appears in future prompts. Adapter must wrap history entries in tags treated as data, same as `<scope>`. Workflow rules likewise.
- **Workflow-rule abuse**: REMIT.md is user-typed and trusted, but a rule could instruct the LLM to leak history into the reason. Mitigation: standard data-only framing + reason redaction guidance from TN-2026-012 §4.
- **Log schema change**: pass-1 summary added to classification log records. Already user-content-derived; same redaction posture as today's action args. The summary may inadvertently capture scope-adjacent phrasing — adapter prompt explicitly forbids restating the scope.
- **Two failure modes compound** — independent `Unavailable` rates multiply. Operator-visible: more `Red(ClassifierUnavailable)` exits. Argues for caching / fallback, not against the two-pass shape.

## Out of scope

- Specific mitigations for the 2× cost (separate plan TN if promoted).
- SQLite-backed history (existing skill `38-sqlite-logger`; orthogonal).
- Cross-**Agent** history (parent-child lineage workflow checks). Lineage hardening is TN-005 deferred work; revisit after this lands.
- Hard-coded skill detection (e.g. matching `/kick-off`). Explicit non-goal — rules must be runtime-agnostic.
- New domain term ratification — defer to a follow-up CDR if this TN is promoted.

## Risks

- **Cost and latency double.** Real product impact; spike before adoption.
- **Compounding Unavailable rates.** Two independent network calls.
- **Workflow rules in natural language are imprecise.** Expect false Greens and false Reds; tune via REMIT.md authoring guidance.
- **History token bloat.** Long sessions could push older relevant events out of the window. Budget tuning is a parameter, not a fix.
- **Vocab churn.** "Procedure" / "Workflow rule" land in `UBIQUITOUS_LANGUAGE.md`; existing TNs untouched but new authors must learn the split from **Scope**.
- **Temptation to skill-match.** First time a rule fails to fire because the LLM didn't recognise an "equivalent" of `/kick-off`, someone will propose hardcoding. Resist — that's a runtime coupling.

## Open questions

- Should pass 1's output be the **only** representation of the action that pass 2 sees, or should pass 2 still get the truncated raw args? Recommend: **both** — summary primary, raw args as fallback evidence. Single-source-of-truth would let a hostile summary fully launder the action.
- Should workflow rules live in `REMIT.md` or a sibling `WORKFLOW.md`? Recommend `REMIT.md` section to minimise file proliferation; promote if/when rules grow large.
- Is `HistorySnapshot` keyed on **Agent** (current session only) or **Agent lineage** (current + ancestors)? Recommend current-only for v1; lineage is TN-005-deferred territory.
