# Developer context

The developer is new to Rust. They are fluent in TypeScript, C#, and other mainstream languages, so general programming concepts, OO patterns, generics, async, and tooling vocabulary are all familiar — **only Rust-specific syntax and concepts need explaining**, and only when they ask.

When they ask for an explanation: be concrete, contrast with TS/C# where useful (e.g. "this is like `Option<T>` — closer to TS `T | undefined` than to C# nullable"), and avoid restating ideas they already know from other languages.

Don't pre-emptively explain Rust idioms in code reviews or while making changes. Wait for the question.

# Vocabulary

[`docs/UBIQUITOUS_LANGUAGE.md`](docs/UBIQUITOUS_LANGUAGE.md) is the project's vocabulary contract. Use those terms exactly in code, docs, commits, and conversation. Notable: **Scope** = the agent's task (not an allowlist), **Verdict** = Green/Red, **Subagent** = any child agent (runtime-agnostic). The code currently still uses pre-rename names (`Decision`, `SpawnId`); follow the doc, those renames are tracked drift.

# Project

Rust workspace. Hexagonal architecture (loosely — will tighten over time).

```
crates/
├── domain/             # pure types + functions; no ports yet
├── file-logger/        # outbound adapter — append-only NDJSON log
└── claude-code-hook/   # inbound adapter — Claude Code PreToolUse hook
```

`claude-code-hook` reads the hook payload from stdin, builds a `ClassificationInput`, and runs it through `domain::Classifier` (wired with `FileLogger`). The classifier logs the input then returns a **Verdict**. Exit code: `0` for Green, `2` for Red (reason on stderr).

`domain::Classifier::classify` currently always returns `Green`. Real rules to come.

# Documentation

- [`docs/tn/`](docs/tn/) — Technical Notes (proposals, specs, plans, exploration, research). Working surface.
- [`docs/decisions/adr/`](docs/decisions/adr/) — Architectural Decision Records. Ratified architecture.
- [`docs/decisions/cdr/`](docs/decisions/cdr/) — Convention Decision Records. Ratified style/tooling/layout.
- [`docs/UBIQUITOUS_LANGUAGE.md`](docs/UBIQUITOUS_LANGUAGE.md) — vocabulary contract.
- [`docs/diag/`](docs/diag/) — empirical hook payload traces (reference only).

See [TN-2026-001](docs/tn/TN-2026-001-technical-notes-and-doc-structure.md) for the doc system itself: numbering, front matter, status semantics, and the TN style rule (compact, bulleted, no long prose).

# Build

```sh
cargo build
just check          # fmt + clippy + tests (mirrors the pre-push hook)
just test           # tests only
```

After cloning, install the pre-push hook:

```sh
./scripts/install-hooks.sh
```

# Workflow

Trunk-based — commits land directly on `main`. The pre-push hook (`scripts/pre-push`) runs `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace`; that's the final quality gate. Never bypass with `--no-verify` — fix the underlying issue.

Issues live on the `remit-sh/Remit` GitHub Project (#9). Each commit references its issue with `Refs #N` (or `Closes #N` to auto-close).
