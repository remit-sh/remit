---
name: do-work
description: Master workflow entry point for any development work — code or docs. Runs the full stage sequence (kick-off → plan → develop → review → push → close), adjusts reviewers by change kind, and supports mid-stream resume. Use this at the start of every piece of work in a trunk-based repo.
disable-model-invocation: false
---

# do-work

**Single entry point for development work in a trunk-based repo.** Any change — new feature, bug fix, refactor, documentation, ADR, TN/PN — runs through this skill. Stages are the same; the reviewer set and extras adjust for the kind of change.

## Triggers

`/do-work`, "start work", "pick up", "pick this up", "let's do this", "develop this"

---

## Stages

Run in order. Don't skip. Each stage is its own skill — `do-work` is the orchestrator.

| #   | Stage          | Skill                  | Code | Docs         |
| --- | -------------- | ---------------------- | ---- | ------------ |
| 1   | Kick-off       | `/do-kick-off`         | ✓    | ✓            |
| 2   | Plan           | `/do-plan`             | ✓    | ✓ (lighter)  |
| 3   | Develop        | (write the code / doc) | ✓    | ✓            |
| 4   | Review         | `/do-review`           | ✓    | ✓ (adjusted) |
| 5   | Streamline doc | `/streamline-doc`      | —    | ✓            |
| 6   | Ship           | `/do-ship`             | ✓    | ✓            |
| 7   | Close          | `/do-close`            | ✓    | ✓            |

---

## Detect change kind

At stage 1, read the issue or scope. Classify:

- **Code change** — modifies source code (`src/`, `packages/`, `apps/`, tests, config)
- **Doc change** — only modifies documentation (`docs/`, `README*.md`, ADRs, TNs, PNs, CDRs, CLAUDE.md / AGENT.md)
- **Mixed** — both. Run the full code path; add `/streamline-doc` before push.

If unclear, ask the user at kick-off.

---

## Review stage adjustments

`/do-review` scales by change kind:

| Reviewer            | Code                             | Docs                                     |
| ------------------- | -------------------------------- | ---------------------------------------- |
| Architect           | When architectural implications  | Always for ADRs / high-level design docs |
| Security            | Auth, permissions, data handling | Skip                                     |
| Code quality        | Always                           | Skip                                     |
| Test coverage       | Always                           | Skip                                     |
| Ubiquitous language | Naming in code                   | Always for docs (they set the terms)     |

Doc-only reviews should still check: cross-references, placement per ADR-014, writing style (BLUF, active voice, short sentences), and freshness of linked material.

---

## Streamline doc (stage 5, docs only)

Before push, run `/streamline-doc` on every doc touched. Enforces the repo's writing-style rules and removes em-dash clutter. Non-negotiable for docs.

---

## Resume mid-stream

If work has already started ad hoc, don't restart. Detect current state and back-fill missing stages:

1. Worktree exists? Skip kick-off Stage 2 (worktree). Confirm issue state is In Progress; if not, advance it.
2. Implementation partly or fully done? Skip develop. Run local review from the current diff.
3. Not yet pushed? Continue at review.
4. Pushed but not closed? Continue at close.

Never skip review and push just because code is written — those are the quality gates.

---

## Autonomous mode (under start-team)

- No confirmation gates _except_ the push-approval gate — `do-ship` Stage 2 always requires explicit approval. Approver identity is set by **project CLAUDE.md** (`## Ship policy`); some projects allow team-lead approval in team sessions, others require human approval even in team sessions. The shared `do-ship` reads this — do not second-guess.
- Report blockers via `SendMessage` to coordinator, don't wait
- Each spawned agent runs `/do-work <issue>` as its single task

---

## Rules

- **Every piece of work runs through `do-work`.** The only exceptions are trivial one-liners the user explicitly scopes as "just do this inline," and ephemeral edits to memory / settings.
- **Stages are a single source of truth** — detail lives in the stage skills. `do-work` is an orchestrator; don't duplicate stage content here.
- **Don't skip review or push stages.** Even for docs. Even when it feels obvious.
- **Inline vs sub-agent is ergonomics.** Run `/do-work` directly in the current session for one issue with visibility; spawn via `start-team` for parallel batch work. Same stages, same skills, either way.
