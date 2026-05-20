# Developer context

- New to Rust. Fluent in TypeScript, C#, and other mainstream languages.
- Explain only Rust-specific syntax and concepts, and only when asked.
- Explanations: concrete, contrast with TS/C# where useful (e.g. `Option<T>` is closer to TS `T | undefined` than to C# nullable). Never pre-emptively explain Rust idioms in reviews or changes.

# Behaviour

**IMPORTANT:** When reporting information to me, be extremely concise and sacrifice grammar for the sake of concision.

## Think before coding

- State assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them — don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## Simplicity first

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If 200 lines could be 50, rewrite.
- Senior-engineer test: "Would they call this overcomplicated?" If yes, simplify.

## Surgical changes

- Touch only what the task requires. Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- Unrelated dead code: mention, don't delete.
- Remove imports/variables/functions your changes orphaned. Don't remove pre-existing dead code unless asked.

## No backwards compatibility

- The project is pre-release. There are no external consumers, no shipped releases, no deprecation contracts to honour.
- When renaming or restructuring: change every callsite to the new name. No deprecated aliases, no `#[deprecated]` shims, no re-exports of old names, no "TODO: remove after migration" comments.
- The codebase should always reflect its current, intended state — never carry transitional layers.

## Naming

- Names should make it clear what a variable is. Reflect the type when not obvious from immediate context.
- Prefer descriptive over terse, even where Rust convention permits short names. A composition root threading multiple stores is clearer with `scope_store` / `pending_scope_store` than `scopes` / `pending`. `payload` beats `p`.
- Short names (`s`, `v`, `i`) are fine only when the variable's whole use is constrained to its declaration line (or at most ~3 lines).

## Goal-driven execution

- Transform tasks into verifiable goals before starting:
  - "Add validation" → "Write tests for invalid inputs, then make them pass"
  - "Fix the bug" → "Write a test that reproduces it, then make it pass"
  - "Refactor X" → "Ensure tests pass before and after"
- For multi-step tasks, state a brief plan with a verify step per item.
- Strong success criteria let you loop independently. Weak criteria require constant clarification.

# Vocabulary

[`docs/UBIQUITOUS_LANGUAGE.md`](docs/UBIQUITOUS_LANGUAGE.md) is the project's vocabulary contract. Use those terms exactly in code, docs, commits, and conversation.

Notable:

- **Scope** — the agent's task (not an allowlist).
- **Verdict** — Green or Red.
- **Subagent** — any child agent, runtime-agnostic.

# Project

Rust workspace, hexagonal architecture.

```
crates/
├── domain/             # pure types + functions; ports
├── file-classification-history/ # outbound adapter — NDJSON write + query
├── claude-code-hook/   # inbound adapter — Claude Code PreToolUse hook
└── remit-cli/          # inbound adapter — `remit` CLI (`remit logs`)
```

`claude-code-hook` reads the hook payload from stdin, builds a `ClassificationInput`, and runs it through `domain::Classifier` (wired with `FileClassificationHistoryWriter`). The classifier records the input to the **Classification history** then returns a **Verdict**. Exit `0` for Green, `2` for Red (reason on stderr). `Classifier::classify` currently always returns Green.

# Architecture rules

Strict hexagonal. Dependency rule:

```
Adapter → Domain    ✅  always
Domain  → Adapter   ❌  never
Adapter → Adapter   ❌  never
```

- Domain has zero infrastructure deps. Trait definitions, domain types, services only.
- Ports are traits in domain. Adapter-agnostic — designed from the domain's perspective.
- Adapters live in separate crates, named by role, not technology.
- Composition roots are the only place that wires adapters to ports.
- Wire types (e.g. hook payload shapes) live in adapter crates, never in domain.

# Security

This is a security tool. Its own posture must be exemplary.

- **Fail-closed.** On any error or uncertainty — parse failure, classifier failure, unknown state — the **Verdict** is Red. Never default to Green.
- **Validate at every boundary.** Hook payloads, CLI args, file contents, IPC messages. Allowlist patterns over denylists.
- **No secrets on disk.** API keys via env vars at runtime. Sensitive content (tool input payloads, scope text beyond a truncated summary) never logged.

# Documentation

- [`docs/decisions/`](docs/decisions/) — ADRs and CDRs. **Active records are authoritative.** Review agents check every change against the `## Compliance` section of each active record, and either flag a non-compliance or draft a new ADR/CDR for human approval rather than silently diverging.
- [`docs/tn/`](docs/tn/) — Technical Notes: proposals, specs, plans, explorations, research.
- [`docs/UBIQUITOUS_LANGUAGE.md`](docs/UBIQUITOUS_LANGUAGE.md) — vocabulary contract (ratified by [CDR-0001](docs/decisions/cdr/CDR-0001-ubiquitous-language-contract.md)).
- [`docs/diag/`](docs/diag/) — empirical hook payload traces.

[CDR-0004](docs/decisions/cdr/CDR-0004-doc-system-layout.md) defines the doc system: numbering, front matter, status, promotion, and the style rule (compact, bulleted, no long prose).

When code changes affect documented behaviour, update the doc in the same change. Stale docs are worse than no docs.

**Run `/streamline-doc` on every newly created or substantially edited markdown doc, immediately after writing.** Applies to TNs, ADRs, CDRs, READMEs, and any other `.md` under `docs/`. Non-negotiable — drafts ship through streamline before review.

# Build

```sh
cargo build
just check          # fmt + clippy + tests (mirrors the pre-push hook)
just test           # tests only
```

After cloning, install the pre-push hook:

```sh
./scripts/install-git-hooks.sh
```

# Test coverage

Aim for near-100% coverage. Every public function and trait implementation gets a unit test. Every port gets an integration test exercising a real adapter, not just mocks. Bar tightens toward 100% as the codebase grows.

Tests live in separate files, never appended to source modules. Unit tests for private items go in a sibling `tests.rs` declared as `#[cfg(test)] mod tests;`. Integration tests live under `tests/`.

# Workflow

- Trunk-based; commits land directly on `main`.
- **Human-approval gate before push.** Every change — even trivial ones — runs through `do-review`, gets committed, has its issue flipped to **In Review**, and waits for the human's explicit go-ahead. Coordinators / team leads never approve pushes on the human's behalf.
- Pre-push hook (`scripts/pre-push`) runs fmt-check, clippy `-D warnings`, and `cargo test --workspace`. Final automated gate, runs after human approval.
- Never bypass with `--no-verify`; fix the underlying issue.
- Issues live on the `remit-sh/Remit` GitHub Project (#9). Reference with `Refs #N` (or `Closes #N` to auto-close).

## Issue sizing

- Prefer one larger issue over several small ones, especially for speculative future work.
- Sub-features of a larger feature belong inside that feature's body (or as a checklist on it), not as separate issues.
- Split only when the work is about to start and parallelism or independent review actually pays off.
- Tiny premature issues create overwhelm without unlocking anything.

## Worktrees

- Always create with a new branch: `git worktree add <path> -b <branch> main`.
- Path convention: `<project>-worktrees/<issue>-<slug>`.
- Never check out `main` (or any primary branch) inside a worktree — locks the primary worktree out.
