---
number: 0001
title: Strict hexagonal architecture with domain-owned ports
status: active
issue: "#16"
sources: ["CLAUDE.md#architecture-rules", "crates/domain/src/lib.rs"]
tags: [architecture, dependency-rule, ports]
---

# ADR-0001: Strict hexagonal architecture with domain-owned ports

## Context

- Remit is a security tool. The classification core must be testable in isolation, IO-free, and replaceable per host (Claude Code today, other agents later).
- Crates already split hexagonally: `domain/`, `file-classification-history/`, `file-scope-store/`, `file-standing-guidance/`, `swift-prompter/`, `osascript-prompter/`, `anthropic-classifier/`, `prompt-lock/`, `remit-config/`, `claude-code-hook/`, `remit-cli/`.
- Existing ports: `ClassificationHistoryWriter`, `ClassificationHistoryReader`, `LlmClassifier`, `AgentStore`, `Prompter`, `PromptLock`, `StandingGuidanceReader` — one MVP adapter each, more expected.

## Decision

Strict hexagonal. **Zero exceptions.**

### Dependency rule

```
Adapter → Domain    ✅  always
Domain  → Adapter   ❌  never
Adapter → Adapter   ❌  never
```

- Domain has zero infrastructure deps; trait definitions, domain types, domain services only.
- Adapters live in their own crate, typically named **`<tech>-<role>`** (`file-classification-history`, `anthropic-classifier`, `swift-prompter`). Adapters are by nature tech-specific — tech in the name is fine. The role-naming rule applies to domain ports (traits), not adapter crates.
- Wire types (hook payloads, HTTP DTOs) live in adapter crates; never leak into domain.
- Composition roots (binary crates, e.g. `claude-code-hook`) are the only place adapters get wired to ports.

### Ports

- Every port is a trait in `crates/domain/src/lib.rs` (or a submodule).
- Port shapes describe what the domain needs, not what any specific adapter provides.
- Errors are domain enums (`ScopeStoreError`, `LlmClassifierError`, `PromptError`) carrying boxed source errors; adapters chain natives without leaking types.
- Ports flow as generic parameters (`fn f<P: ScopePrompter>(...)`), not `dyn`. Keeps the boundary monomorphic and zero-cost. `dyn` requires a documented reason.
- Adapters implement domain traits; no parallel trait hierarchies.

## Consequences

- New capabilities arrive as a new port in domain + a new adapter crate. No shortcut path.
- Cross-adapter calls are forbidden; shared behaviour gets pulled up into domain or duplicated.
- Trait shape changes ripple to every adapter — the desired forcing function.
- Domain tests run without IO setup; domain uses in-memory fakes, not `mockall`-style mocking. Adapter tests run against real backends.

## Compliance (review agents)

- New crate dependencies on `domain/` are fine; any other inter-crate dep is a flag.
- `domain/Cargo.toml` must stay free of infrastructure deps (no `serde_json`, `tokio`, `ureq`; no `tempfile` outside dev-deps).
- Wire / payload structs (`#[derive(serde::Deserialize)]` for hook payloads, HTTP requests) belong in adapter crates, not domain.
- Composition wiring (`Classifier::new`, `process_action(...)`) appears only in binary crates.
- A new port lands in `crates/domain/src/lib.rs`, not in an adapter. Trait signatures reference only domain types: no `serde_json::Value`, `PathBuf`-shaped wire data, or adapter error types.
- Trait method names describe a domain capability (`scope_for`, `assign`, `classify`), not an adapter mechanism (`read_file`, `post_request`).
- Domain code takes ports as generics; `dyn` needs a documented reason.
- Any `domain → adapter` or `adapter A → adapter B` import is a Red verdict; restructure or draft a follow-up ADR.
