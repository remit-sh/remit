---
number: 2026-002
title: MVP North Star
kind: spec
status: draft
issue: "#15"
tags: [mvp, vision, classifier, scope, cli]
---

# TN-2026-002: MVP North Star

> End state for the MVP. Each component spawns its own implementation issue; decomposition tracked under #15.

## Goal

- Every Claude Code session has a scope.
- Every tool call classified by an LLM against the **root** session's scope.
- Subagents inherit via parent walk.
- Project-level `remit.md` and role templates shape the classifier.
- Logs queryable via a `remit` CLI.

## Locality

- **Project-level** is the default: templates, `remit.md`, anything that shapes classification.
- **User-level** (`~/.remit/`) only for cross-project state: `dev_bypass`, runtime paths, SQLite store location.

## Components

### 1. Session-start scope prompt

- Hook fires on session start; if no scope, system dialog prompts; answer persisted against the session id.
- TBC: `SessionStart` hook vs first `PreToolUse`.

### 2. Scope inheritance

- Subagents and agent-team children inherit the **root** session's scope.
- Resolution walks parent ids until an owned scope is found.

### 3. LLM classifier

- Every `PreToolUse` classified by Claude Code Haiku.
- Inputs: tool name, tool input, root scope, project `remit.md`, applicable templates.
- Output: **Verdict** (Green | Red); Red carries a reason.
- Fail-closed on any classifier error.

### 4. `remit.md` at project root

- Read on every classification.
- Standing instructions for the classifier — **not** an allowlist.
- Example: "issues live on GitHub Projects" lets the classifier place `gh issue create` in context for a developer scope.

### 5. Project templates

- Role templates (`developer.md`, `team-lead.md`, …) under `<repo>/.remit/templates/`.
- Concatenated onto the session's scope before classification.
- TBC: exact path; selection mechanism; precedence vs scope and `remit.md`.

### 6. SQLite log + `remit` CLI

- Classifications append to a SQLite store; replaces today's NDJSON logger.
- Columns: timestamp, session id, root session id, tool name, scope summary, verdict, reason, `dev_bypass`.
- `remit logs` (and friends) for querying.

## Non-goals (MVP)

- Allowlist/denylist syntax.
- Network telemetry.
- Cross-machine scope sharing.
- Hosts other than Claude Code.

## Open questions

- Prompt UI delivery (system dialog vs hook output vs CLI).
- Template precedence and concatenation order.
- Classifier failure UX — silent Red or surface error.
- `remit.md` lookup: git-rooted or cwd-rooted.

## Relationship to existing issues

- #9 chain-walk → component 2.
- #10 logger captures error + verdict → absorbed by component 6.
- #11 Verdict::Red structured kind → supports component 3.
- #12 runtime paths → prerequisite for component 6.
- #14 `dev_bypass` audit marker → absorbed by component 6.

## Promotion criteria

- Spec decomposed into per-component issues with acceptance criteria.
- No open question blocks implementation start.
