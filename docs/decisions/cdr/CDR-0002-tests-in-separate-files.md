---
number: 0002
title: Tests live in separate files
status: active
issue: "#16"
sources: ["CLAUDE.md#test-coverage"]
tags: [convention, tests, layout]
---

# CDR-0002: Tests live in separate files

## Context

- Rust convention often appends `#[cfg(test)] mod tests { ... }` at the bottom of the source file.
- For non-trivial suites, this buries production code under test setup and makes diffs noisy.

## Decision

- Tests **never** live inline at the bottom of a source file.
- Unit tests for items in `lib.rs` (or any `mod.rs`) live in a sibling `tests.rs`, declared `#[cfg(test)] mod tests;` in the source file.
  - Example: `crates/file-classification-history/src/lib.rs` ↔ `crates/file-classification-history/src/tests.rs`.
- Integration tests (anything exercising adapters end-to-end against real backends) live under each crate's `tests/` directory.
- Domain-side contract test harnesses (shared across adapter test suites) live in `crates/domain/src/test_support.rs` behind the `test-support` feature flag (see CDR-0003).

## Consequences

- Diffs in production files stay readable.
- Test files can grow without drowning the source.
- Sibling `tests.rs` is discoverable via the `mod tests;` declaration; no hidden coupling.

## Compliance (review agents)

- A `#[cfg(test)]` block containing actual test functions inline at the bottom of a source file is a flag; move to a sibling `tests.rs` (or `tests/<name>.rs` for integration scope).
- The `mod tests;` declaration must be `#[cfg(test)]`-gated so test code never compiles into release builds.
- A new crate without a `tests/` directory is a flag if it ships an adapter (adapters need integration tests against their real backend, per CDR-0003).
- `mockall`-style heavyweight test scaffolding in `src/` is a flag; test support belongs behind the `test-support` feature in domain.
