---
number: 2026-003
title: Session scope acquisition
kind: plan
status: draft
issue: "#17"
tags: [scope, prompter, hook, mvp]
---

# TN-2026-003: Session scope acquisition

> Plan for #17. Slice covers MVP components 1 and 2 of [TN-2026-002](TN-2026-002-mvp-north-star.md).

## Goal

- Every Claude Code tool call has a scope to classify against.
- User sets scope through a system UI dialog popped by the hook itself. No file editing, no CLI.
- Subagents (both topologies) inherit the **root** session's scope.

## Already done

- `Session { id, subagent_id, parent_id }` domain type.
- `ScopeReader` / `ScopeWriter` ports.
- `FileScopeStore` adapter (one `<id>.json` per session).
- `resolve_scope` — single hop: session, then `parent_id`.
- `Classifier::classify` — Green if scope present, Red ("no scope set") if absent, Red ("unavailable") on error.
- Hook builds `parent_id` from `CLAUDE_SESSION_ID`, which always points at root (see [`docs/diag/`](../diag/README.md)).

Inheritance is already correct for both topologies: single hop suffices because the env var is fork-time-set to root, even for nested team-spawn → in-process chains.

## Gaps

1. **Prompt mechanism**: no way to ask the user for a scope. Today the Red reason is just `"no scope set for this session"`.
2. **Persistence wiring**: `FileScopeStore::assign` exists but nothing in the runtime calls it.
3. **Scope dir**: `claude-code-hook/main.rs` hardcodes a compile-time path (`concat!(env!("CARGO_MANIFEST_DIR"), "/../../scopes")`). Fine for the build tree, wrong for installed use.
4. **Reason taxonomy**: a bare `String` reason can't differentiate "you cancelled the prompt" from "your subagent's parent has no scope" from "the prompter is broken." All three need different remediation.

## Design

### 1. Synchronous system dialog from the hook

When the hook detects no scope on a root session, it shells out to `osascript` to pop a native macOS dialog with a text field. The user types the scope, the hook writes it, then returns Green so the original tool call proceeds.

```
osascript -e '
  display dialog "Set scope for this session"
    default answer ""
    with title "Remit"
    buttons {"Cancel", "Set scope"}
    default button "Set scope"
'
```

- Cancel → Red `ScopeSetupCancelled`.
- Empty input → treated as cancel.
- `osascript` missing or fails → fail-closed Red `ScopeSetupUnavailable`.

The future custom UI replaces this adapter; the port stays.

### 2. New port: `ScopePrompter`

```rust
// in domain
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum PromptError {
    #[error("prompt cancelled by user")]
    Cancelled,
    #[error("prompt unavailable")]
    Unavailable(#[source] Box<dyn std::error::Error + Send + Sync>),
}

pub trait ScopePrompter {
    fn prompt(&self, session_id: &SessionId) -> Result<Scope, PromptError>;
}
```

Adapter-agnostic. `Cancelled` is distinct from `Unavailable` so the verdict reason and metrics can tell them apart.

### 3. New adapter crate: `osascript-prompter`

- `crates/osascript-prompter/`.
- One impl: `OsascriptPrompter` implementing `ScopePrompter`.
- Spawns `osascript`, parses stdout / exit status:
  - `osascript` exits 1 with stderr `User canceled` → `PromptError::Cancelled`.
  - Empty answer → `PromptError::Cancelled`.
  - Spawn / IO failure → `PromptError::Unavailable`.
  - Successful answer → `Scope::new(trim(answer))`.

No timeout for MVP. Risk: user walks away, Claude Code times the hook out. Acceptable until we own the UI.

### 4. New domain function: `acquire_scope`

The acquisition flow is a separate domain function — **not** absorbed into `Classifier`. The hook composition root calls it before `classify`, so `Classifier` stays a pure query (state → verdict).

```rust
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum AcquireScopeError {
    #[error("scope store: {0}")]
    Store(#[from] ScopeError),
    #[error("prompt: {0}")]
    Prompt(#[from] PromptError),
}

pub fn acquire_scope<R, P, W>(
    reader: &R,
    prompter: &P,
    writer: &W,
    session: &Session,
) -> Result<Option<Scope>, AcquireScopeError>
where
    R: ScopeReader,
    P: ScopePrompter,
    W: ScopeWriter,
```

Logic:

1. `resolve_scope(reader, session)?` → if `Some(s)`, return.
2. Else, if `session.has_parent()`, return `Ok(None)` (subagent inheritance failed; can't prompt mid-subagent).
3. Else (root session, missing scope), `prompter.prompt(&session.id)`:
   - `Ok(scope)` → `writer.assign(&session.id, scope.clone())?`, return `Ok(Some(scope))`.
   - `Err(PromptError::Cancelled)` → `Ok(None)`.
   - `Err(PromptError::Unavailable(_))` → propagate as `AcquireScopeError::Prompt(...)`.

Lives in `domain` so other hosts (Cursor, Codex) reuse it verbatim.

### 5. Shared parent predicate

Extract `Session::has_parent()` to keep `resolve_scope` and `acquire_scope` consistent on the self-referential-parent rule:

```rust
impl Session {
    pub fn has_parent(&self) -> bool {
        self.parent_id.as_ref().is_some_and(|p| p != &self.id)
    }
}
```

`resolve_scope` switches to `session.has_parent()`; existing `ignores_self_referential_parent` test still passes.

### 6. Verdict gains a typed `RedKind`

Bare `String` reasons drift; the taxonomy belongs in one place.

```rust
pub enum Verdict {
    Green,
    Red { kind: RedKind, reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedKind {
    NoScopeRoot,                  // root session, no scope, no prompt attempted
    ScopeSetupCancelled,          // user dismissed the prompt
    ScopeSetupUnavailable,        // prompter failed (osascript missing, IO error)
    SubagentParentMissingScope,   // subagent's parent walk found nothing
    StoreUnavailable,             // ScopeError from reader/writer
}
```

`reason` keeps the human-readable text; `kind` enables structured logging and the per-host UX.

### 7. Classifier — stays reader-only

`Classifier::classify` keeps its current shape and dependency set (`L: ClassifierLogger`, `R: ScopeReader`). No new generics, no acquisition logic. It maps presence → Green and absence/error → Red with the right `RedKind`:

- `Ok(Some(_))` → Green.
- `Ok(None)` with parent → `SubagentParentMissingScope`.
- `Ok(None)` without parent → `NoScopeRoot` (in practice, the hook acquires before classifying, so this is the "user just cancelled" path; the hook overrides the kind to `ScopeSetupCancelled` based on the acquisition outcome — see §8).
- `Err(_)` → `StoreUnavailable`.

### 8. Composition root (`claude-code-hook`)

```text
let outcome = acquire_scope(&store, &prompter, &store, &session)?;
let verdict = classifier.classify(&input);
let verdict = match (outcome, verdict) {
    (None, Verdict::Red { kind: NoScopeRoot, .. }) =>
        Verdict::Red { kind: ScopeSetupCancelled, reason: "scope setup cancelled".into() },
    (_, v) => v,
};
```

`AcquireScopeError::Prompt(Unavailable)` and `AcquireScopeError::Store(_)` are handled by the hook by emitting Red with `ScopeSetupUnavailable` / `StoreUnavailable` respectively, mirroring the existing fail-closed pattern.

### 9. Scope dir → `~/.remit/scopes/`

- Add `scope_dir() -> Result<PathBuf, ConfigError>` to `remit-config`. Default `~/.remit/scopes/`.
- Hook resolves it at startup. Adapter-side concern; doesn't affect the port.
- No file-config override yet.

## Touch list

- `crates/domain/src/lib.rs` — `ScopePrompter`, `PromptError`, `AcquireScopeError`, `acquire_scope`, `Session::has_parent`, `RedKind`, updated `Verdict`. `Classifier` only changes its Red-emission path to attach `RedKind`.
- `crates/domain/tests/acquire_scope.rs` — six paths from §4.
- `crates/domain/tests/classifier.rs` — assert `RedKind` on existing scenarios; existing assertions adapted.
- `crates/domain/tests/resolve_scope.rs` — switch to `has_parent` (no behavioural change).
- `crates/domain/tests/common/mod.rs` — `FakePrompter` (configurable scope, error, call count); `RecordingWriter`.
- `crates/osascript-prompter/Cargo.toml` — new adapter crate.
- `crates/osascript-prompter/src/lib.rs` — `OsascriptPrompter` impl. Inject the command builder so unit tests stub `osascript`.
- `crates/osascript-prompter/tests/integration.rs` — opt-in macOS test (`#[ignore]`) that pops a real dialog.
- `crates/remit-config/src/lib.rs` — `scope_dir()`.
- `crates/remit-config/src/tests.rs` — `HOME` set / `HOME` missing.
- `crates/claude-code-hook/src/main.rs` — wire `OsascriptPrompter`, `FileScopeStore` (reader + writer); call `acquire_scope` then `classify`; map `RedKind` and `AcquireScopeError` to exit code + stderr. Replace hardcoded scope path with `remit_config::scope_dir()`.
- `crates/claude-code-hook/Cargo.toml` — add `osascript-prompter` dep.
- `Cargo.toml` (workspace) — add `osascript-prompter` member.
- Issue #17 — pointer comment + test strategy.

`remit.log` paths and the `dev_bypass` flow are untouched.

## Test strategy (TDD)

Failing test first, then implement. Order:

1. **`remit-config`** — `scope_dir_under_home`, `scope_dir_errors_when_home_missing`.
2. **Domain — `Session::has_parent`** — true for distinct parent, false for none, false for self-referential.
3. **Domain — `acquire_scope`**:
   - Returns existing scope without prompting or writing.
   - Returns parent's scope without prompting or writing (subagent path).
   - Subagent with no parent scope → `Ok(None)`, prompter and writer not called.
   - Root, missing scope → calls prompter, writes via writer, returns the new scope.
   - Cancelled prompt → `Ok(None)`, writer not called.
   - Unavailable prompt → `Err(AcquireScopeError::Prompt(_))`, writer not called.
   - Store error → `Err(AcquireScopeError::Store(_))`.
4. **Domain — `Classifier`** — each branch attaches the correct `RedKind`. Existing Green case still passes.
5. **`osascript-prompter`** — unit tests over the parser (success line, "User canceled" stderr, IO failure). Integration `#[ignore]` test gated on macOS pops a real dialog.
6. **End-to-end smoke** (manual, recorded in commit): fresh session → first tool call shows dialog → typed scope persists → next call Green → Cancel returns Red with the cancelled-kind text.

Coverage target per CLAUDE.md: every public function and trait impl unit-tested; every port covered by an adapter integration test.

## Out of scope (per #17)

- LLM classifier; `remit.md`; templates.
- SQLite logging.
- `SessionStart` interactive prompt.
- Custom Remit UI (osascript stand-in until then).
- Non-macOS platforms. Cross-platform prompter ships when there's a host that demands it.
- Config-file override for `scope_dir`.
- Hook-timeout protection for slow user input.
- Concurrent-prompt race (two parallel root sessions both with no scope).
- `Scope` length / content validation.

## Open questions

None blocking. The two notes from architect review (concurrent prompts, `Scope` validation) are recorded as future issues.

## Promotion criteria

- All acceptance criteria in #17 pass with automated tests.
- End-to-end smoke recorded in the commit message.
