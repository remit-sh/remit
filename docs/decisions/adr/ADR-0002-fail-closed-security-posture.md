---
number: 0002
title: Fail-closed security posture
status: active
issue: "#16"
sources: ["CLAUDE.md#security", "TN-2026-004", "TN-2026-005"]
tags: [architecture, security, verdict]
---

# ADR-0002: Fail-closed security posture

## Context

- Remit is a semantic firewall. A Green verdict permits an action; a Red verdict blocks it.
- Failure modes (parse errors, IO failures, network errors, missing config, unknown state) are inevitable.
- Defaulting to Green would fail-open. Unacceptable for a security tool.

## Decision

- On **any** error or uncertainty, the verdict is **Red**.
- Applies at every layer:
  - Boundary parse failures (hook payload, ids, config) → exit 2 with `remit: <ctx>: <err>; deny`. See `claude-code-hook::fail_closed`.
  - Port errors (`ScopeStoreError`, `PromptError`, `LlmClassifierError`) map to typed Red verdicts (`StoreUnavailable`, `ScopeSetupUnavailable`, `ClassifierUnavailable`).
  - Missing scope on a subagent → `SubagentMissingScope` (Red); never silent inheritance.
  - Empty or whitespace-only scope text rejected at the boundary (`Scope` newtype refuses construction).
- Logging is **best-effort**: a logging glitch must never flip Red to Green, nor block a verdict. `FileLogger` swallows IO errors by design.
- Composition roots translate verdicts to transport (exit 2 + stderr for the hook); translation never re-introduces a fail-open path.

## Consequences

- Trade-off: operator pain on broken config / network is louder than fail-open silence. `dev_bypass` is the single sanctioned escape hatch; audit-marked `[dev-bypass-active]` on every log line.
- Every new error site needs an explicit `RedKind`. If none fits, widen the port contract or the enum.
- Adapter tests must exercise failure paths, not just happy paths.

## Compliance (review agents)

- `unwrap_or_default()` / `unwrap_or(false)` / `.ok()` discarding errors at a security boundary is a flag.
- Any path where an error returns `Verdict::Green` is a flag.
- New port errors must map to a `RedKind` (existing or new) at the composition layer.
- New boundary inputs (CLI args, env vars, file contents, IPC messages) must validate at the boundary; allowlist over denylist.
- Logging failures must not propagate; classification failures must not be swallowed.
- Only `dev_bypass` may turn Red into permit; it must keep emitting the would-have-denied reason and the audit marker.
