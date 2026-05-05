---
number: 2026-004
title: ClassifierLogger captures error and verdict
kind: plan
status: active
issue: "#10"
tags: [domain, classifier, observability, fail-closed]
---

# TN-2026-004: ClassifierLogger captures error and verdict

> Extend the outbound observability port. Audit logs record the verdict, plus the underlying error when scope acquisition fails (currently discarded into a Red Verdict).

## Problem

- `ClassifierLogger::log_incoming` records the input only.
- `process_action` discards `AcquireScopeError` into `Verdict::Red`. `format!("{e}")` keeps the top-level Display; the source chain is lost.
- The Verdict itself is not logged. Audit trail shows what came in, not what was decided.

## Design

### Trait — three observability hooks

```rust
pub trait ClassifierLogger {
    fn log_incoming(&self, input: &ClassificationInput);
    fn log_outcome(&self, input: &ClassificationInput, verdict: &Verdict);
    fn log_error(&self, input: &ClassificationInput, error: &(dyn std::error::Error + 'static));
}
```

- No default impls. Every adapter and test fake updates explicitly.
- `log_error`'s `'static` bound enables source-chain walking and downcasting.

### Where each hook is called

- `log_incoming` — once per `process_action` call, at the top.
- `log_error` — once when `AcquireScopeError` is discarded.
- `log_outcome` — once per `process_action` call, at the end, with the **final** Verdict (after any `NoScopeRoot → ScopeSetupCancelled` transformation).

`Classifier::classify` is pure — it decides, no side effects. `process_action` owns the audit trail end to end.

### `Classifier` becomes a unit struct

Logger generic and field dropped. `Classifier::new()` takes no args. Existing logger generic was only there to feed `classify`'s old `log_incoming` call.

### `process_action` signature

Takes `&Classifier` and `&L: ClassifierLogger` separately:

```rust
pub fn process_action<R, P, W, L>(
    input: &ClassificationInput,
    scope_reader: &R,
    prompter: &P,
    scope_writer: &W,
    classifier: &Classifier,
    logger: &L,
) -> Verdict
```

### `process_action` shape

```rust
logger.log_incoming(input);
let verdict = match acquire_scope(...) {
    Err(e) => {
        logger.log_error(input, &e);
        match e {
            AcquireScopeError::Prompt(pe) => Verdict::Red { kind: ScopeSetupUnavailable, reason: format!("scope prompt unavailable: {pe}") },
            AcquireScopeError::Store(se)  => Verdict::Red { kind: StoreUnavailable,       reason: format!("scope store unavailable: {se}") },
        }
    }
    Ok(scope) => match classifier.classify(input, scope.as_ref()) {
        Verdict::Red { kind: NoScopeRoot, .. } => Verdict::Red {
            kind: ScopeSetupCancelled,
            reason: "scope setup cancelled; nothing was set".into(),
        },
        v => v,
    },
};
logger.log_outcome(input, &verdict);
verdict
```

## FileLogger format

- One line per event, matching the existing `kind=value` style.
- `outcome:` line carries `kind` (Green or RedKind) and the reason for Red.
- `error:` line walks `Error::source()` chain and joins with `: ` so the inner cause is preserved.

## Test plan

- TDD: add failing tests for `log_outcome` and `log_error`, then implement.
- `domain/tests/classifier.rs` — unchanged assertions on `log_incoming`; classify still logs incoming.
- `domain/tests/process_action.rs` — add:
  - `log_outcome` called once per call with final verdict (including `ScopeSetupCancelled` after transformation).
  - `log_error` called once on `AcquireScopeError::Store` and `::Prompt`; not called on Green.
- `domain/tests/common/mod.rs` — extend `RecordingLogger` to track all three.
- `file-logger` — exercise `log_error` source-chain rendering.

## Out of scope

- Verdict reason format. Reason stays a brief Display; full chain lives in the log.
- NDJSON-vs-text-format docstring drift in `FileLogger` (pre-existing).
- Tool-args sensitivity in `log_incoming` (pre-existing; tracked separately).

## Files touched

- `crates/domain/src/lib.rs`: trait, `Classifier` unit struct, `process_action` rewiring.
- `crates/file-logger/src/lib.rs`: `log_outcome` and `log_error` impls.
- `crates/domain/tests/common/mod.rs`: `RecordingLogger` tracks all three events.
- `crates/domain/tests/classifier.rs`: drop logger from test setup.
- `crates/domain/tests/process_action.rs`: pass logger; new assertions.
- `crates/claude-code-hook/src/main.rs`: `Classifier::new()`; pass logger to `process_action`.
- `docs/tn/TN-2026-003-session-scope-acquisition.md`: list all three trait methods.
- `docs/tn/README.md`: index entry for this TN.
