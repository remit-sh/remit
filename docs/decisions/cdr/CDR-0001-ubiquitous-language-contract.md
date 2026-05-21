---
number: 0001
title: Ubiquitous language is the vocabulary contract
status: active
issue: "#16"
sources: ["CONTEXT.md", "CLAUDE.md#vocabulary"]
tags: [convention, vocabulary]
---

# CDR-0001: Ubiquitous language is the vocabulary contract

## Context

- Remit's vocabulary is dense (Scope, Verdict, Agent, Subagent, Bundled scope, …) and aliases creep in fast (task, intent, permissions, session, ruling).
- `CONTEXT.md` (formerly `docs/UBIQUITOUS_LANGUAGE.md`) is treated as authoritative in `CLAUDE.md`. Renamed 2026-05-20 to align with the shared `ubiquitous-language` skill convention.

## Decision

- `CONTEXT.md` (at repo root) is the project's vocabulary contract.
- The terms there are used **exactly** in code, doc comments, commits, conversation, issue titles, and PR descriptions. Listed aliases are not used.
- New terms enter the lexicon by editing `CONTEXT.md` first, in the same change that introduces them.
- Domain types link back to the vocabulary file in their doc comments (existing pattern: `Vocab: [Scope](../../../CONTEXT.md#product)`).
- Remit is a single-context project — no per-package contexts and no `CONTEXT-MAP.md`. The shared skill's multi-context model does not apply here.

## Consequences

- Code review treats vocabulary drift as a real defect, not a stylistic preference.
- Renames ripple through the codebase rather than living as silent local aliases.
- Cross-runtime concepts (Agent vs. Session) stay disambiguated when we add non-Claude-Code adapters.

## Compliance (review agents)

- Introducing a banned alias (`task`, `intent`, `permissions`, `engine`, `decision`, `ruling`, `inherited scope`, …) is a flag.
- New domain types or ports must add a `Vocab:` doc-comment link to the relevant section of `CONTEXT.md`.
- A new term in code without a `CONTEXT.md` entry is a flag; add the entry in the same change or rename to an existing term.
- The literal string `hook` without qualification (Claude Code hook vs. git hook) is a flag in user-facing docs, error messages, and commit messages.
