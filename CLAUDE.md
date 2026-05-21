# Developer context

New to Rust. Fluent in TypeScript, C#. Explain Rust syntax only when asked. Don't pre-emptively explain idioms in reviews.

# Behaviour

**Be extremely concise. Sacrifice grammar for concision.**

## Think before coding
- State assumptions. Ask if uncertain.
- Multiple interpretations → present, don't pick silently.
- Simpler approach → say so. Push back when warranted.
- Unclear → stop, name it, ask.

## Simplicity first
- No features, abstractions, or flexibility beyond what was asked.
- No error handling for impossible scenarios.
- 200 lines that could be 50 → rewrite.
- Senior-engineer test: "would they call this overcomplicated?" If yes, simplify.

## Surgical changes
- Touch only what the task requires. Don't improve adjacent code. Don't refactor what isn't broken. Match existing style.
- Unrelated dead code: mention, don't delete.
- Orphaned imports/vars from your changes → remove. Pre-existing dead code → leave.

## No backwards compatibility

- The project is pre-release. There are no external consumers, no shipped releases, no deprecation contracts to honour.
- When renaming or restructuring: change every callsite to the new name. No deprecated aliases, no `#[deprecated]` shims, no re-exports of old names, no "TODO: remove after migration" comments.
- The codebase should always reflect its current, intended state — never carry transitional layers.

## Naming
- Names reflect what the variable IS. `scope_store` > `scopes`, `payload` > `p`.
- Short names (`s`, `v`, `i`) only when scope ≤ 3 lines.

## Goal-driven execution
- Tasks → verifiable goals (e.g. "Add validation" → "Write tests for invalid inputs, then make them pass").
- Multi-step: brief plan with a verify step per item.

# Vocabulary

[`CONTEXT.md`](CONTEXT.md) — vocabulary contract, ratified by [CDR-0001](docs/decisions/cdr/CDR-0001-ubiquitous-language-contract.md). Use those terms exactly in code, docs, commits, and conversation.

# Project & architecture

Rust workspace, hexagonal. 11 crates:
- **Pure:** `domain/`, `remit-config/`.
- **Inbound (binaries):** `claude-code-hook/` (PreToolUse), `remit-cli/` (`remit logs`).
- **Outbound:** `anthropic-classifier/`, `file-classification-history/` (NDJSON), `file-scope-store/`, `file-standing-guidance/`, `prompt-lock/`, `swift-prompter/`, `osascript-prompter/`.

Strict hexagonal: Adapter → Domain only. Never Domain → Adapter. Never Adapter → Adapter.
- Domain: zero infrastructure deps. Traits, types, services only.
- Ports = traits in domain, designed from the domain's perspective.
- Adapters in separate crates, typically `<tech>-<role>` (tech in name is fine — they're tech-specific by nature).
- Composition roots are the only place adapters wire to ports.
- Wire types (hook payloads, etc.) live in adapter crates, never in domain.

# Security

- **Fail-closed.** Any error or uncertainty → Verdict is Red. Never default to Green.
- **Validate at every boundary.** Hook payloads, CLI args, files, IPC. Allowlist over denylist.
- **No secrets on disk.** API keys via env vars at runtime. Tool input payloads and scope text never logged beyond a truncated summary.

# Documentation

- [`docs/decisions/`](docs/decisions/) — ADRs / CDRs. **Active records authoritative.** Every change is checked against `## Compliance`; non-compliance is flagged or a new record drafted for approval. Never silently diverge.
- [`docs/tn/`](docs/tn/) — Technical Notes. [`docs/diag/`](docs/diag/) — hook payload traces. Layout: [CDR-0004](docs/decisions/cdr/CDR-0004-doc-system-layout.md).
- [`REMIT.md`](REMIT.md) — Project guidance, read by the Classifier on every call. Treat as policy, not prose.
- **Repo state:** `.claude/` = policy, skills, settings (checked in). `.agent-state/` = runtime per-clone state, e.g. approval marker (gitignored). Never invert.
- Code change affects docs → update in the same change. **`/streamline-doc` on every new or substantially edited `.md` immediately after writing.** Non-negotiable.

# Test coverage

Near-100%. Every public fn + trait impl: unit test. Every port: integration test against a real adapter. Tests in separate files; private-item tests in a sibling `tests.rs` declared `#[cfg(test)] mod tests;`; integration tests under `tests/`.

# Build

`just check` (= fmt + clippy + tests; mirrors pre-push). `just test`. After cloning: `./scripts/install-git-hooks.sh`.

Two enforcement layers: Claude Code hooks via `.claude/settings.json` (active on clone — approval gate + rustfmt-on-save) + git `pre-push` via `./scripts/install-git-hooks.sh` (manual install — fmt + clippy + tests).

# Workflow

- Trunk-based, direct to `main`. Pre-push hook (fmt + clippy `-D warnings` + tests) runs after human approval. **Never `--no-verify`.**
- Issues: `remit-sh/Remit` Project #9. Reference with `Refs #N` (or `Closes #N` to auto-close).

## Ship policy

Read by `do-ship`. Authoritative.

- **Approver:** human only. Coordinators / team leads route the human's go-ahead, never substitute. Silence ≠ approval.
- **Commit timing:** commit *after* approval. Flow: `do-review` → human reviews uncommitted diff → approval → commit → push.
- **Approval signals:** `approve`, `push`, `ship`, `lgtm`, `go`, `👍`, optionally with `#N`. Ambiguous → ask. Changes requested → apply, re-run `do-review`, return to Stage 1.
- **Scope:** every change, even trivial. No skip path.
- **Approval marker:** after approval + commit, write `.agent-state/last-approval.json` `{"head_sha":"<HEAD>","approved_at":"<ISO 8601>","approver":"human"}`. PreToolUse hook `scripts/claude-check-approval.sh` blocks `git push` if missing or SHA-stale.

## Issue sizing

Prefer one larger issue over several small. Sub-features belong in the parent's body/checklist. Split only when work is about to start and parallelism pays off.

## Worktrees

`git worktree add <path> -b <branch> main` (new branch always). Path: `<project>-worktrees/<issue>-<slug>`. Never check out `main` (or any primary branch) inside a worktree — locks the primary worktree out.
