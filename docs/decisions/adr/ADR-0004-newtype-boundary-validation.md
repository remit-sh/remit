---
number: 0004
title: Boundary validation via newtypes
status: active
issue: "#16"
sources: ["CLAUDE.md#security", "crates/domain/src/lib.rs"]
tags: [architecture, validation, security]
---

# ADR-0004: Boundary validation via newtypes

## Context

- Domain types must arrive at the classifier already valid; the classifier shouldn't re-check what the boundary enforced.
- Strings flowing in from hook payloads, env vars, files, and adapter responses are untrusted until validated.
- `nutype` gives validated newtypes; cheap to construct, zero runtime overhead in release.

## Decision

- Every domain identity / value type with invariants is a `nutype` newtype with `validate(...)`:
  - `AgentId` — non-empty, ≤128 chars, ASCII alphanumeric / `-` / `_`.
  - `Scope` — non-empty after trim.
  - `CallId`, `ActionKind`, `ActionArgs` — opaque newtypes (no validation today; the boundary wraps adapter strings so future invariants can be added without ripple).
- Construction is the boundary: `try_new` returns the validation error; `new` (where present) is for already-trusted callers.
- Adapters call `try_new` at ingest. Failure → fail-closed (ADR-0002): exit 2 with `remit: <ctx>: <err>; deny`.
- Domain functions take newtypes by reference (`&Scope`, `&AgentId`); never raw `&str` / `String` for fields with a domain newtype.

## Consequences

- Invariant violations surface at one well-known site (the boundary), not deep inside business logic.
- Adapter code is slightly more verbose at the edges; the interior is provably valid.
- Adding an invariant (e.g. AgentId character set) is a one-place change; existing call sites already route through `try_new`.

## Compliance (review agents)

- Any new domain field carrying a string with invariants needs a newtype wrapper, not bare `String`.
- Boundary code producing a domain-newtype value (payload deserialisation, env var lookup, file content) must use `try_new` and fail-closed on `Err`.
- Domain signatures take the newtype, not its inner type: `fn f(scope: &Scope)`, never `fn f(scope: &str)`.
- Tests for new newtypes must include the invariant violations they're supposed to catch.
- Removing `validate(...)` from an existing newtype is a breaking change and requires its own ADR.
