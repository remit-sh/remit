---
number: 0003
title: Single composition root per adapter binary
status: active
issue: "#16"
sources: ["crates/claude-code-hook/src/main.rs"]
tags: [architecture, composition]
---

# ADR-0003: Single composition root per adapter binary

## Context

- Hexagonal architecture (ADR-0001) requires adapter wiring in one place per deployment surface.
- Today: `claude-code-hook` only (Claude Code `PreToolUse` binary). Future surfaces (Cursor adapter, CLI runner, daemon) each get their own binary crate.
- Hook process is short-lived: spawned per tool call, reads stdin, classifies, exits. State lives on disk.

## Decision

- One binary crate per host integration. Today: `crates/claude-code-hook`.
- Each binary's `main` is the **only** site that instantiates and wires adapters. Library crates never construct other adapter crates.
- The composition root owns:
  - Reading host config (`remit_config::load`).
  - Translating host transport (hook payload JSON, exit codes, env vars) into domain types.
  - Building each adapter (`FileLogger::new`, `FileScopeStore::new`, `OsascriptPrompter::new`, `AnthropicClassifier::from_config`, …).
  - Calling the domain entry point (`process_action`, `claim_and_assign`).
  - Translating the resulting `Verdict` back into host transport (exit 2 + stderr for the hook).
- Adapter selection (`ClassifierAdapter::Anthropic` → `AnthropicClassifier`) lives in `build_llm_classifier`. New variants force a new match arm; exhaustive enum is the discipline.

## Consequences

- A new host integration means a new binary crate, not new library code.
- Library crates stay free of host glue: constructors take what they need; the binary fills them.
- Composition-root tests are binary-level integration (stdin → exit code). Wiring is intentionally thin, verified end-to-end.

## Compliance (review agents)

- Adapter `::new` calls outside `crates/<binary>/src/main.rs` (or its module tree) are a flag; wiring belongs in the composition root.
- Library crates import only `domain`; no library may import another adapter crate.
- Host transport (exit codes, hook event names, env var lookup) lives in the binary, not in adapter crates.
- New host-integration code in an existing adapter crate is a flag; grow a new binary instead.
- The composition root's `match` on adapter selectors must remain exhaustive (no `_ =>` arm) so a new `ClassifierAdapter` variant forces explicit wiring.
