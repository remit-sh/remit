---
number: 2026-011
title: Swift prompter binary — UI foundation
kind: plan
status: active
issue: "#43"
tags: [prompter, ui, swift, ipc]
---

# TN-2026-011: Swift prompter binary

> Replace the osascript-driven `Prompter` adapter with a native Swift binary that owns all current and future user-facing UI (verdict dialogs now; tray icon, dashboards, "recently blocked" timeline next). Rust hook talks to it over JSON on stdin/stdout per invocation.

## Problem

- Big Sur+ NSAlert is fundamentally a narrow portrait box: centered text, ≥3 buttons always stack vertically, icon clamped to a small square area. No public API restores pre-Big Sur layout. The current `osascript-prompter` has hit that ceiling.
- AppleScript-ObjC inside `osascript -e ...` is a poor authoring medium for anything more than NSAlert: no autocomplete, fragile delegates, opaque errors.
- The product trajectory needs a real macOS app (menu-bar item, dashboard, recently-blocked timeline). Building it in osascript is a dead end; building it in Swift now is the foundation.

## Decision

Ship a native Swift CLI binary `remit-prompter` that runs custom NSWindow dialogs and exits. Rust hook spawns it per invocation, pipes one JSON message in, reads one JSON message out. Two binaries side-by-side on disk:

```
~/.local/bin/remit-claude-code-hook   (Rust, the hook entry point)
~/.local/bin/remit-prompter           (Swift, owns all UI)
```

Future `Remit.app` bundle subsumes `remit-prompter` as `Remit.app/Contents/MacOS/remit-prompter`; the Rust adapter changes its locator and nothing else.

## Why not pure Rust via objc2

- `objc2-app-kit` works but is unergonomic for UI code (every property access is `unsafe { msg_send![] }`). Custom layout, focus handling, key equivalents, image loading — all noisy.
- We've committed to a future Remit.app GUI anyway; the Swift binary IS that codebase's seed. Investing in the Rust-Cocoa path now means rewriting it in Swift later.

## Why not embed-and-extract a Swift binary inside the Rust binary

- Hides a real second binary behind tempfile gymnastics. Adds Gatekeeper / quarantine surprises on signed builds.
- Once we ship `Remit.app`, the second binary is intentional and visible. The embed-extract trick was for hiding a helper; we're not hiding anything.

## Wire format

One JSON object per stdin, one JSON object per stdout. UTF-8, no trailing newline required. Every request carries `"protocol": 1`; bumping is a hard break (handshake mismatch → adapter fails closed without invoking UI).

Responses are tagged with `kind` matching the request — the adapter rejects any reply whose `kind` doesn't match, defending against wire drift / version skew silently misrouting one verdict variant into another's slot.

### `choose`

```jsonc
// stdin
{
  "protocol": 1,
  "kind": "choose",
  "scope": "Agenthoff dev work",
  "reason": "Action accesses ~/.ssh ...",
  "working_dir": "/Users/paul.grimshaw/dev/agenthoff/como"
}
// stdout — exactly one of:
{ "kind": "choose", "choice": "deny" | "allow" | "update_scope" | "disable_session" }
{ "kind": "choose", "cancelled": true }
```

### `update_scope`

```jsonc
// stdin
{
  "protocol": 1,
  "kind": "update_scope",
  "current": "Agenthoff dev work",   // omitted for first-time setup
  "working_dir": "/Users/paul.grimshaw/dev/agenthoff/como"
}
// stdout
{ "kind": "update_scope", "scope": "Agenthoff dev work — focus on como" }
{ "kind": "update_scope", "cancelled": true }
```

### `confirm_disable`

```jsonc
// stdin
{ "protocol": 1, "kind": "confirm_disable" }   // no working_dir — Prompter::confirm_disable has none
// stdout
{ "kind": "confirm_disable", "confirmed": true | false }
```

### Hard rules (binary)

- The Swift binary MUST emit either a complete JSON response or nothing at all on stdout. No partial writes; flush exactly once before exit. If a panic / unrecognised request kind occurs, emit nothing on stdout and a one-line message on stderr, then exit non-zero.
- The Swift binary MUST drain stdin fully before showing UI. Truncated requests → exit 2.

### Hard rules (adapter)

- On non-zero exit: **discard stdout entirely** (it may contain partial / leaked content). Map to `PromptError::Unavailable` with stderr in the source chain.
- On zero exit: parse stdout as JSON; validate the response shape **matches the request kind** (e.g. `confirm_disable` may not return `{"choice": ...}`). Shape mismatch → `Unavailable`.
- Cap stdin write to 64 KiB and stdout read to 64 KiB. Exceeded → kill child, `Unavailable`.
- Wall-clock timeout of 5 minutes per call. Exceeded → kill child + drain, `Unavailable`. (Dialogs are user-blocking; a real user takes seconds, not minutes. 5 min is generous enough to cover a distracted user without wedging the hook permanently.)

### Cancellations vs errors

Cancellations are NOT errors — they are explicit `{"cancelled": true}` / `{"confirmed": false}` responses with exit 0. The adapter maps these to `Err(PromptError::Cancelled)` / `Ok(false)` respectively.

### Forward compatibility

- Unknown keys in requests/responses ignored on both sides (Swift uses `Codable`; Rust uses `serde(deny_unknown_fields = false)` — the default).
- New dialog kinds are additive. Adapter whitelists known kinds; Swift binary whitelists known kinds and exits 2 on unknown (with a stderr message). Adapter handles exit 2 same as any non-zero — fail-closed Red.
- `protocol` bumps are deliberate breaks. Both sides log a clear message on mismatch.

### Version skew

Both binaries emit `--version` (semver string baked at compile time). Adapter logs `prompter-version-skew` event when its own version differs from the binary's; non-fatal in v1 (both halves still attempt the request) — promoted to a hard error once we have a release cadence to align them.

## Components

### `swift/` SwiftPM project (new)

```
swift/
├── Package.swift                     SwiftPM, executable target RemitPrompter
└── Sources/RemitPrompter/
    ├── main.swift                    parse stdin, dispatch on kind, emit stdout
    ├── Wire.swift                    Codable request/response structs
    ├── Branding.swift                Wordmark NSImage from embedded asset; colours; metrics
    ├── ChooseDialog.swift            NSWindow: wordmark + body + horizontal NSButton row
    ├── UpdateScopeDialog.swift       NSWindow: wordmark + body + NSTextView + Save/Cancel
    └── ConfirmDisableDialog.swift    NSWindow: wordmark + body + Cancel/Disable
```

Layout pattern (shared across dialogs):

```
+-------------------------------------------------------+
|  [WORDMARK 240×56]                                   |   ← header band, leading-aligned
|  ------------------------------------------------     |   ← thin separator
|  ~/dev/agenthoff/como                                |   ← cwd row, monospaced, dim
|                                                       |
|  Reason: Action accesses ~/.ssh directory ...        |   ← body, wraps
|                                                       |
|  Scope: Agenthoff dev work                           |
|                                                       |
|  [Deny]  [Allow (a)]  [Update scope (u)]  [Disable (s)]
+-------------------------------------------------------+
```

- Fixed window width ~560pt (enough for 4 buttons in a row).
- Vertical stack via NSStackView (orientation: .vertical, alignment: .leading, spacing: 12).
- Button row is a horizontal NSStackView at the bottom (orientation: .horizontal, alignment: .centerY, spacing: 8). Trailing alignment per macOS HIG.
- Deny is `keyEquivalent = "\r"` (default, highlighted blue). Allow/Update/Disable get single-letter key equivalents matching the labels.
- Window style: `[.titled, .closable]`, `level = .modalPanel`. Owns its event loop via `NSApp.runModal(for:)`.
- App activates itself: `NSApp.setActivationPolicy(.accessory)` + `NSApp.activate(ignoringOtherApps: true)`, same trick we already use in osascript adapter.

### `crates/swift-prompter/` (new Rust adapter)

```
crates/swift-prompter/
├── Cargo.toml
├── src/lib.rs                        impl Prompter for SwiftPrompter
└── tests/integration.rs              uses a fake binary script
```

`SwiftPrompter`:

- Constructor `SwiftPrompter::new()` resolves the binary path:
  1. `$REMIT_PROMPTER_BIN` if set (test override).
  2. Sibling of `argv[0]` named `remit-prompter` (works when both binaries live in `~/.local/bin`).
  3. Bare `remit-prompter` via `$PATH` lookup as last resort.
  Path resolution is lazy: the actual `Command::new` happens per-call, so swapping the binary on disk is picked up without restart.
- Per-call:
  1. Spawn the binary, capture stdin/stdout/stderr.
  2. Write JSON request to stdin (≤ 64 KiB; oversize is a panic — internal bug, not user input).
  3. Wait with a 5-minute wall-clock budget; on expiry kill child, drain pipes, return `Unavailable`.
  4. On exit: if non-zero → discard stdout, return `Unavailable` with stderr in source chain.
  5. On zero exit: parse stdout JSON; validate response shape matches request kind; return parsed `VerdictChoice` / `Scope` / `bool`.
- Stdout is **never** embedded in error chains (could leak `update_scope` text).
- Implements all three `Prompter` methods with shared plumbing; only the wire-message kind differs.

### `crates/claude-code-hook/` (composition root update)

Single-line swap: `OsascriptPrompter::new()` → `SwiftPrompter::new()`. Rest of the composition root is untouched (`PromptingDeps`, the lock, the store, the logger).

### `crates/osascript-prompter/` (delete)

After the swap is verified end-to-end: remove the crate, drop its workspace member entry, drop the dependency from `claude-code-hook`. The Rust adapter trait surface (`Prompter`) stays unchanged — that's the win.

### `justfile`

- New recipe `swift-build`: `cd swift && swift build -c release`.
- New recipe `install`: builds both binaries in release mode, copies to `~/.local/bin/`. Replaces the manual `swap-binary.sh` dance.
- `check` recipe: adds `swift-build` to the chain so CI catches Swift breakages alongside Rust.

## Distribution

- macOS-only. SwiftPM relies on the Xcode command-line tools; standard developer machine has this.
- Both binaries unsigned for now (same posture as today). When `Remit.app` work starts, the bundle gets a developer-ID signature and notarisation; `remit-prompter` lives inside.
- No tempfile extraction, no embedded blobs.

## Visual design

- **Wordmark**: re-use existing `crates/osascript-prompter/assets/remit-wordmark.png`; copy into `swift/Sources/RemitPrompter/Resources/Wordmark.png` and load via `Bundle.module`. Same wordmark, finally rendered at its intended size.
- **Body typography**: system body (`NSFont.systemFont(ofSize: NSFont.systemFontSize)`); cwd row uses `NSFont.userFixedPitchFont(ofSize: NSFont.smallSystemFontSize)`.
- **Colours**: defaults (so light/dark mode just works). No custom palette in v1 — that's a Remit.app-era polish.
- **Scope edit (`update_scope`)**: NSTextView in a bordered NSScrollView, ~480×88pt, monospaced. Return key inserts a newline; Cmd-Return saves. Save and Cancel buttons in the same horizontal row as the other dialogs.

## Test strategy

TDD where it makes sense; visual dialogs are inherently manual.

### Swift unit tests (`swift/Tests/RemitPrompterTests/`)

- `Wire` Codable round-trips for all three request kinds + all response variants.
- Forward-compat: extra unknown keys in request JSON do not break decoding.
- Malformed JSON → exit 2 + stderr message; covered by a single test that runs the binary with bad input.

### Rust unit tests (`crates/swift-prompter/src/tests.rs`)

- `SwiftPrompter` constructs request JSON correctly for each method call (capture via fake binary that echoes its stdin).
- Parses each response variant correctly (choose × 5 outcomes, update_scope × 2, confirm_disable × 2).
- Non-zero exit → `PromptError::Unavailable`; stderr preserved in source chain.
- Stdout never appears in error messages (would leak scope text).
- Binary missing entirely (path resolves to non-existent file) → `Unavailable`. **Fail-closed proof.**
- Hung child (sleeps past timeout) → child killed, `Unavailable`, no hang.
- Oversized stdout (binary emits > 64 KiB) → kill, `Unavailable`.
- Response-kind mismatch (binary emits `{"confirmed": true}` to a `choose` request) → `Unavailable`.
- Stdout-on-non-zero-exit is silently discarded — assert it never appears in the error chain.
- `protocol` field is 1 on every request emitted.

### Rust integration test (`crates/swift-prompter/tests/integration.rs`)

- Drives a fake `remit-prompter` (a tiny shell script in `target/test-bin/`) through all three methods. Asserts round-trip plumbing without spinning up a real GUI.
- Real-binary integration is manual — verified by running the hook in a Claude Code session.

### Manual smoke (pre-commit, documented)

- Trigger a Red verdict in a real session for each of the three dialog paths. Check: wordmark renders at intended size, buttons are horizontal, focus is correct on the dialog, keyboard shortcuts work, Escape cancels.

## Migration plan

Two landings, deliberately split — a regression in the swap can be reverted with one line.

**Landing A** (this issue):
1. TN + GitHub issue.
2. Scaffold `swift/` SwiftPM project; pass `cargo test` (no changes to Rust yet).
3. Build the three dialogs with placeholder content; verify each renders on dev machine.
4. Implement JSON wire (Wire.swift + main.swift dispatch).
5. Build `crates/swift-prompter/`; unit tests with fake binary.
6. Swap composition root from `OsascriptPrompter` to `SwiftPrompter` behind a single change in `claude-code-hook/src/main.rs`.
7. Manual smoke of all three dialogs against the new prompter.
8. Update `justfile` (`swift-build`, `install`, extended `check`).
9. Update `docs/CONFIGURATION.md` to document the two-binary layout.

`osascript-prompter` stays in tree at end of Landing A — out of the dependency graph but compileable. A regression is one Cargo.toml line to revert.

**Landing B** (follow-up issue, after one usage cycle): delete `crates/osascript-prompter/` and its workspace member entry. No code change beyond crate removal.

## Out of scope (v1)

- `Remit.app` bundle / menu-bar tray icon / dashboard. This TN is the Swift codebase's seed, not its full build-out.
- Code signing & notarisation. Unsigned ad-hoc binary for now; matches existing posture.
- Cross-platform UI. Adapter is macOS-only; another `Prompter` impl can serve linux/tui when needed.
- Persistent "always allow this tool for this scope". v1 Allow stays single-shot.
- Window position memory / multi-monitor placement. NSAlert today doesn't do this either.
- Hard protocol-version enforcement. v1 logs skew; v2 (after release-cadence alignment) fails closed.

## Promoted into v1 from architect review

- 5-minute wall-clock timeout per call.
- 64 KiB stdin/stdout size caps.
- `"protocol": 1` field on every request; `"kind"` tag on every response.
- Adapter validates response `kind` matches request `kind` — defends against wire drift.
- Adapter discards stdout entirely on non-zero exit (no leak into errors).
- Adapter caps stderr at 4 KiB inside error chains (chatty binary can't bloat audit log).
- Binary handshakes WindowServer availability at startup; exits 3 if headless.

## Files touched

- New: `swift/Package.swift`, `swift/Sources/RemitPrompter/*.swift`, `swift/Sources/RemitPrompter/Resources/Wordmark.png`, `swift/Tests/RemitPrompterTests/*.swift`.
- New: `crates/swift-prompter/Cargo.toml`, `crates/swift-prompter/src/lib.rs`, `crates/swift-prompter/src/tests.rs`, `crates/swift-prompter/tests/integration.rs`.
- Modify: `Cargo.toml` (add `swift-prompter` member — `osascript-prompter` membership stays until Landing B), `crates/claude-code-hook/Cargo.toml` (drop `osascript-prompter` dep, add `swift-prompter`), `crates/claude-code-hook/src/main.rs`.
- Modify: `justfile` (`swift-build`, `install`, extended `check`).
- Modify: `docs/CONFIGURATION.md` (binary layout).
- Modify: `docs/tn/README.md` (index entry).
- Delete (Landing B, follow-up issue): `crates/osascript-prompter/` entire crate.

## Risks

- **`swift build` slow first-run** (~30–60s as it fetches SwiftPM dependencies if any added; we use none in v1 = first-run still ~10s). Mitigation: zero external deps.
- **Permission prompts on first run** of an unsigned binary that calls AppKit. Mitigation: same as today's osascript invocation — Gatekeeper has already allowed unsigned `osascript` calls via the hook; new binary inherits the same TCC posture by being invoked from the same parent.
- **Crash in Swift binary** under user input. Mitigation: any non-zero exit fails closed to Red via existing `PromptError::Unavailable` → fail-closed Verdict::Red path. The hook never wedges; worst case the user sees a Red.
- **CI without Xcode CLT**: tests that touch the Swift binary are gated on `cfg(target_os = "macos")` + binary presence; CI runners without `swift` skip them with a `eprintln!` warning rather than fail. The `just check` recipe wraps the `swift-build` step with the same skip — `command -v swift >/dev/null && (cd swift && swift build) || echo "swift toolchain absent; skipping"`. CI presence/absence parity with the integration test guard.
- **Headless / SSH session**: when no WindowServer is available (SSH login, launchd context without a UI session), AppKit's `NSApp.run` traps. The Swift binary catches this at startup (`NSApp.setActivationPolicy` returns failure or no Aqua session detected via `NSWorkspace`) and exits 3 with a one-line stderr message; adapter maps as `Unavailable` → fail-closed Red. Same posture as today's osascript-in-SSH behaviour, made explicit.
