---
number: 0003
title: Near-100% coverage with port contract tests
status: active
issue: "#16"
sources: ["CLAUDE.md#test-coverage", "crates/domain/src/test_support.rs"]
tags: [convention, tests, coverage]
---

# CDR-0003: Near-100% coverage with port contract tests

## Context

- Remit is a security tool. Untested code paths are tomorrow's fail-open bugs.
- Port-shape and adapter-shape errors are different categories: a port contract test pins the *shape* of the port; an adapter test pins one specific implementation.

## Decision

- Aim for **near-100% line coverage**. Every public function and trait implementation gets a unit test. The bar tightens toward 100% as the codebase grows.
- Every port (domain trait) has a **contract test harness** in `crates/domain/src/test_support.rs`, gated by the `test-support` Cargo feature. Existing examples: `scope_store_contract`, `pending_scope_store_contract`.
- Every adapter calls the relevant contract harness from a `#[test]` to prove conformance: same suite for every backend. Adapter-specific behaviour (file paths, HTTP shape, osascript output parsing) gets dedicated tests beyond the contract.
- The `test-support` feature is opt-in (e.g. `features = ["test-support"]` in `[dev-dependencies]`) so contract harnesses never ship in release builds.
- Mocks are not the default. Domain tests use in-memory fakes; adapter tests run against real backends (`tempfile`-backed file systems, `tiny_http`-backed HTTP servers, real `osascript` invocations where feasible).

## Consequences

- Adding a new adapter for an existing port is structurally cheap: implement, call the contract harness, add tests for adapter-local behaviour.
- Adding a new port forces a contract harness in the same change; port and conformance suite ship together.
- Test execution stays fast because we don't carry a heavy mocking framework.

## Compliance (review agents)

- A new domain trait without an accompanying contract harness in `test_support.rs` is a flag.
- An adapter implementation that doesn't call the relevant contract harness from its tests is a flag.
- A public function or trait `impl` added without a unit test is a flag.
- `mockall` or similar heavy mocking in `domain/` is a flag; domain tests use plain fakes.
- A drop in coverage on a touched file (relative to its prior state) requires justification in the change description; otherwise it's a flag.
