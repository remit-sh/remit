# REMIT.md

## Project

Remit — classifier-backed hook deciding whether an agent's tool call matches the session scope. Polyglot workspace, hexagonal: a Rust core with adapters in whatever language the integration demands (Swift for the macOS prompter, shell for hooks, more to come). Anything inside this repo is part of the workspace.

## Conventions

- Trunk-based, direct to `main`. Worktrees at `<project>-worktrees/<issue>-<slug>` (sibling of this repo). Never check out `main` in a worktree.
- Hexagonal: Adapter → Domain only. Never Domain → Adapter. Never Adapter → Adapter. Wire types live in adapter crates.
- Vocabulary fixed by `CONTEXT.md` (CDR-0001). Use those terms verbatim in code, docs, commits, chat.
- Fail-closed throughout. Any error or uncertainty → `Verdict::Red`.

## Workflow

Every developer follows the same loop, every time:

- **GitHub-tracked.** All work is an issue on GitHub Project #9. No untracked work. Reference with `Refs #N` or `Closes #N`.
- **`/do-work` is the entry point.** kick-off → plan → develop → review → push → close. Resume mid-stream if interrupted.
- **TDD throughout.** Red → green → refactor. Tests first, then the code that makes them pass. Near-100% coverage. Unit tests beside source (`tests.rs`); integration tests under `tests/`.
- **Approval gate is human-only.** `do-review` → human reviews uncommitted diff → approval → commit → push. Pre-push hook runs after approval. Never `--no-verify`.

A scope set at the start of an issue covers the whole `/do-work` loop for that issue, including `do-review`, `do-ship`, and `do-close`. Shipping the work you're developing isn't a separate concern from developing it.

## Docs are core, not overhead

Doc work is development work. `docs/` is the primary medium of design, decision, and trace; agents read and write it constantly.

- **Read before acting.** Consult ADRs/CDRs (`docs/decisions/`), TNs (`docs/tn/`), and diagnostic traces (`docs/diag/`) before touching code. Active decision records are authoritative; every change is checked against their `## Compliance` section.
- **Write alongside code.** Code change ships with matching doc update in the same change. New feature → TN. Architectural shift → ADR/CDR. Hook payload investigation → `docs/diag/` trace.
- **`/streamline-doc` every new or substantially edited `.md` immediately after writing.** Non-negotiable.
- Doc layout fixed by CDR-0004. Don't invent top-level folders.

## Tooling

- `just check` — fmt + clippy `-D warnings` + tests. Mirrors pre-push hook.
- `just test` — full test run.
- `gh` for all issue and project interaction.

## Agent bookkeeping

A handful of tools are bookkeeping rather than work product, and appear routinely in every session here:

- `TaskCreate`, `TaskUpdate`, `TodoWrite` — the agent organising its own intent.
- `AskUserQuestion` — the agent checking in with the human.
- `ScheduleWakeup` and similar — agent cadence.

Their subject text describes plans, questions, or upcoming work — not what the tool itself does. A `TaskCreate` whose subject mentions an unrelated topic is the agent noting "I should think about X later", not doing X.

## Out of scope

- Code changes without the matching doc update.
- Code written before its test.
- Silent divergence from an active ADR/CDR. Flag it or draft a superseding record for approval.
- Domain code importing adapter crates, or adapters importing each other.
- Logging tool-input payloads or scope text beyond a truncated summary. No secrets on disk.
- Bypassing the approval gate (`scripts/claude-check-approval.sh`) or pushing without a fresh `.claude/last-approval.json`.
- Editing `main` inside a worktree. Worktrees in hidden directories.
- Work outside a GitHub issue.
