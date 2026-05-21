---
number: 2026-014
title: Swift prompter hang — surface-budget timeout + actionable Red
kind: plan
status: active
issue: "#45"
tags: [prompter, ui, swift, ipc]
---

# TN-2026-014: Swift prompter hang fixes

> Hook freezes for 300 s when the Swift prompter UI doesn't surface. Hit once during a 3-session team spawn; not reproduced since. Split the per-call timeout into two phases — short budget to *surface* the dialog, then indefinite wait for the user. Add a user-actionable hint to the Red message on prompter unavailability. Defer the global lock / foreground-activation work until the race recurs.

## Problem

- Hook fires on `PreToolUse`, spawns `remit-prompter`. Window doesn't appear.
- After 300 s the adapter times out, returns Red. Session is unusable during the freeze; recovery requires editing `~/.claude/settings.json` to disable the hook.
- Triggered once on a 3-session parallel `do-work` team spawn (all three first-time scope setup, all three timed out). Not reproduced since.
- A flat per-call timeout punishes slow users as much as non-surfacing dialogs — they're indistinguishable from the adapter's side.

## Decision

Two fixes now; two structural fixes documented for later.

### 1. Two-phase timeout (ship)

Split the single 300 s timeout into two phases keyed on a "ready" signal from the Swift binary.

- **Surface phase: 30 s.** Adapter spawns the binary, writes the request, waits for either child exit *or* a `READY\n` line on stderr. If the deadline expires with no `READY` and no exit, kill the child and return `Unavailable` → Red.
- **Respond phase: indefinite.** Once `READY` is observed, the adapter waits for the child to exit with no further timeout. User can step away from their machine; dialog stays open.

Wire change: Swift binary writes `READY\n` to stderr **after** `window.makeKeyAndOrderFront(nil)` and **before** `NSApp.runModal(for: window)`. Centralised in `Dialog.assemble` so all three dialog kinds emit it.

Adapter changes (`crates/swift-prompter/src/lib.rs`):
- Set the child's stderr fd to `O_NONBLOCK`; poll alongside `child.try_wait()`.
- Scan accumulated stderr for the `\nREADY\n` (or leading `READY\n`) marker.
- Replace `DEFAULT_TIMEOUT` with `SURFACE_TIMEOUT = 30 s`. No total-call timeout.
- On surface-phase timeout: kill the child, drain remaining stderr, return `Unavailable("prompter did not surface within 30s")`. Surface-phase stderr is discarded except for the captured READY (no leakage; secret content can only be on stdout per existing rules).
- On any pre-READY exit (binary crashed before showing UI): treat as non-zero exit, map to `Unavailable` with stderr in the chain. Existing rule.

Forward-compat: an older Swift binary that doesn't emit READY would always hit the surface timeout. Both binaries ship in lockstep, so this is acceptable — same posture as the protocol field.

### 2. Actionable Red on prompter unavailability (ship)

When `Verdict::Red { kind: RedKind::ScopeSetupUnavailable, .. }` reaches the exit-2 path in `claude-code-hook/src/main.rs`, append a recovery hint:

```
scope prompt unavailable: prompter did not surface within 30s

To bypass while debugging, set `dev_bypass = true` in ~/.remit/config.toml.
```

Restricted to `ScopeSetupUnavailable` — genuine out-of-scope Reds must not suggest dev-bypass.

### 3. Global UI serialisation (deferred)

If the multi-session race recurs:

- File lock at `<remit_home>/prompter.lock`, acquired in `SwiftPrompter::run` before spawn, RAII-released.
- Lives in the `swift-prompter` adapter as a private helper (not a domain port). `FsPromptLock` covers per-agent re-classification; this is cross-agent UI ownership — a different concern, scoped to macOS GUI adapters.

### 4. Foreground activation (deferred)

If single-session surfacing fails:

- Try cheap softer steps first: `window.orderFrontRegardless()` after `makeKeyAndOrderFront(nil)`, and `NSRunningApplication.current.activate(options: [.activateAllWindows, .activateIgnoringOtherApps])` as a second-pass activate.
- Last resort: `.accessory` → `.regular` activation policy (brief dock-icon flash, reliable surface).

## Components

- **`swift/Sources/RemitPrompter/Dialog.swift`** — After `window.makeKeyAndOrderFront(nil)` in `Dialog.assemble`, emit `READY\n` to stderr and flush. One line.
- **`crates/swift-prompter/src/lib.rs`**
  - Rename `DEFAULT_TIMEOUT` → `SURFACE_TIMEOUT`, value 30 s.
  - In `run`: set stderr fd to `O_NONBLOCK`; loop polls stderr + `try_wait` until either `READY` seen, child exits, or surface deadline expires.
  - Post-`READY`: drop the deadline; wait for child to exit (still polling stderr for capture, just no time limit).
  - Keep existing 64 KiB caps, non-zero-exit handling, response-kind validation, stdout-on-non-zero-exit discard.
- **`crates/swift-prompter/src/tests.rs`** — `SURFACE_TIMEOUT == 30 s` constant assertion.
- **`crates/swift-prompter/tests/integration.rs`** — Two new scenarios via fake binaries:
  - Binary emits `READY\n` to stderr then sleeps 1 s then writes JSON → adapter returns the parsed response (proves indefinite wait works post-ready).
  - Binary never emits `READY` (just sleeps) → adapter kills at surface timeout, returns `Unavailable`.
  - Existing `hung_child_is_killed_after_timeout` becomes "never-surfaces" semantics.
- **`crates/claude-code-hook/src/main.rs`** — When the Red kind is `ScopeSetupUnavailable`, append the dev-bypass hint to the exit-2 stderr line. Dev-bypass branch unchanged (already bypassed).
- **`crates/claude-code-hook/src/tests.rs`** — Red with `ScopeSetupUnavailable` ⇒ stderr contains the hint. Red with any other kind ⇒ stderr does **not** contain the hint.

No wire-protocol-version bump. The `READY` stderr marker is forward-compatible-ish: old hooks ignore stderr lines, new hooks expect them, new binaries always emit them — pinned by lockstep release.

## Test strategy

TDD. All paths deterministic via fake binaries.

**`crates/swift-prompter`**:
- `SURFACE_TIMEOUT` constant pinned at 30 s.
- READY then sleep then respond: parsed response returned even though spawn-to-exit > surface timeout.
- No READY (binary sleeps forever): killed at surface timeout, returns `Unavailable`.
- READY then crash (non-zero exit): `Unavailable` with stderr in chain.
- READY in mid-stderr alongside other lines: still detected (line-anchored match).
- Stderr READY marker does not leak into error chains on the non-zero-exit path.

**`crates/claude-code-hook/src/tests.rs`**:
- `ScopeSetupUnavailable` Red → stderr contains the hint.
- Any other Red kind → stderr does **not** contain the hint.

**Manual smoke**:
- Trigger a Red on a fresh session — dialog appears within 1 s, READY visible in `~/.remit/remit.log` event stream.
- Leave the dialog up for 5+ minutes without clicking — adapter does not time out.
- Set `REMIT_PROMPTER_BIN=/bin/sleep` (binary doesn't emit READY) — agent receives Red with the dev-bypass hint within 30 s.

## Risks

- **Stderr READY marker collides with a real "READY" stderr line from the binary.** Mitigated by line-anchored match on a dedicated token. If a future Swift dependency emits `READY\n` for unrelated reasons, false surface-detect → adapter waits indefinitely on what may already be a hang. Acceptable; we control the Swift codebase.
- **`O_NONBLOCK` portability.** macOS-only adapter so Unix `fcntl` is fine. CI gating already conditions integration tests on `cfg(unix)`.
- **READY-then-immediate-exit race.** Binary writes READY, then crashes before adapter polls. Adapter still sees the READY in stderr buffer + an exit status; routes through the standard non-zero-exit path. No special case needed.
- **Future race in a multi-session burst.** Documented in §3; revisit when observed.

## Files touched

- Modify: `swift/Sources/RemitPrompter/Dialog.swift`.
- Modify: `crates/swift-prompter/src/lib.rs`, `crates/swift-prompter/src/tests.rs`, `crates/swift-prompter/tests/integration.rs`.
- Modify: `crates/claude-code-hook/src/main.rs`, `crates/claude-code-hook/src/tests.rs`.
