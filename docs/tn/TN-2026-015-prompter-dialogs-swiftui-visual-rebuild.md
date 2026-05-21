---
number: 2026-015
title: Prompter dialogs — SwiftUI visual rebuild
kind: plan
status: draft
issue: "#54"
tags: [prompter, ui, swift, swiftui]
---

# TN-2026-015: Prompter dialogs — SwiftUI visual rebuild

> Rebuild the visual layer of all three prompter dialogs in SwiftUI, hosted inside the existing NSWindow + runModal shell. AppKit owns activation, focus, modal lifecycle; SwiftUI owns rendering. Dark-mode first, keyboard-first, no dock icon, no tray. Today's "tap, tap, enter" speed preserved.

## Problem

- Current dialogs are visually flat: every row carries the same weight. Reason should anchor the dialog; action should read at a glance.
- TN-2026-012's `<tool_name>: <args>` row truncates mid-content. Long Bash commands unreadable.
- Pure-AppKit polish means hand-rolled NSView subclasses for every tinted surface, capsule, icon row, custom title-bar widget. High labour, low expressiveness.

## Decision

SwiftUI inside `NSHostingView`, embedded as the content view of the existing `NSWindow`. Each dialog file becomes:

1. A SwiftUI `View` (the visual).
2. A thin AppKit wrapper that builds the window, embeds the SwiftUI view, runs the modal, returns the response.

All lifecycle plumbing unchanged: `setActivationPolicy(.accessory)`, `activate(ignoringOtherApps:)`, `runModal(for:)`, `makeFirstResponder`, main menu for Cmd+V/C/X/A/Z.

### Why hosted, not `@main App` + `WindowGroup`

- `.accessory` policy plays badly with the SwiftUI `App` lifecycle (dock flicker, ghost windows, focus grab unreliable).
- CLI lifecycle (read stdin → show one dialog → write stdout → exit) doesn't map to `WindowGroup`'s scene model.
- The proven activation/focus shell is what makes today's UX feel fast. Keeping it eliminates a whole class of regression risk.

### Why not stay pure AppKit

- Every primitive in the mockup (materials, SF Symbols in rows, capsule key hints, custom title bar, picker with badges + destructive-text rows) is SwiftUI-idiomatic.
- AppKit gets there too, but every element is a custom `NSView` subclass with hand-rolled Auto Layout. SwiftUI expresses the same in a fraction of the code.

## Architecture

```
Main.swift                  unchanged — stdin/dispatch/runModal/stdout/exit
Wire.swift                  unchanged — Codable request/response
Dialog.swift                keep ensureAppKitReady, main menu, makeWindow,
                            runModal, CloseAsCancelDelegate. Drop the AppKit
                            field/button builders. Delete ArrowKeyWindow.
DialogChrome.swift   (new)  SwiftUI chrome (DialogTokens, TitleBar,
                            InfoSection<Content>, KeycapHint, DialogFooter,
                            PickerRow) plus the AppKit bridge that drives
                            them (KeyHandlingHostingView).
Branding.swift              + SwiftUI Color/Font tokens.
ChooseDialog.swift          SwiftUI view + thin AppKit wrapper
UpdateScopeDialog.swift     "
ConfirmDisableDialog.swift  "
```

Each `run(_ req:) -> Response` wrapper:

1. Build the `NSWindow` via `Dialog.makeWindow()`.
2. Create the coordinator as a plain reference (`let coordinator = ChooseCoordinator()`). The wrapper owns it for the lifetime of the modal so it can read the response after `runModal` returns.
3. `window.contentView = KeyHandlingHostingView(rootView: SomeDialogView(coordinator: coordinator), onArrow: coordinator.handleArrow, onReturn: coordinator.commit, onEsc: coordinator.cancel)`. The SwiftUI view declares `@ObservedObject var coordinator: ChooseCoordinator` — not `@StateObject` — because the wrapper owns the lifetime.
4. `makeFirstResponder(window.contentView)`.
5. `NSApp.runModal(for: window)`.
6. Return the coordinator's response (defaults to `.cancelled`).

## Visual tokens

Dark-mode first. Light-mode tuning deferred.

| Token | Value |
|---|---|
| App background | `Material.ultraThin` over `Color(red: 0.05, green: 0.07, blue: 0.09)` (deep navy) |
| Surface | `Color.white.opacity(0.05)`, 8pt corner |
| Warning surface | `Color.red.opacity(0.15)`, 1pt `Color.red` stroke, 8pt corner |
| Selection | indigo `Color(red: 0.33, green: 0.41, blue: 1.0)`; text becomes pure white |
| Destructive text | system red |
| Text primary | white |
| Text secondary | `Color.secondary` |
| Caption (section labels) | uppercase, `.caption2.bold()`, tracking +0.5, `.secondary`, sits **outside** the surface |
| Keycap | `Color.white.opacity(0.08)` fill, 1pt `Color.white.opacity(0.15)` stroke, 4pt corner, monospaced label |
| Outer padding | 16pt |

## Title bar

- Centred accessory: small wordmark image (`Branding.wordmark` from existing `Resources/Wordmark.png`, rendered ~14pt high). No text pairing — wordmark already reads as the project name.
- Window style: `[.titled, .closable]`, `titlebarAppearsTransparent = true`, `titleVisibility = .hidden`. Traffic lights left (system).
- Right: custom close `X` via `NSTitlebarAccessoryViewController` (Apple's native API). Routes through `CloseAsCancelDelegate`, same code path as Esc.

## Choose dialog

### Warning banner

- Rounded rectangle, warning tokens.
- `HStack(alignment: .top, spacing: 12)`:
  - `Image(systemName: "hand.raised.fill").foregroundStyle(.red)`
  - `VStack(alignment: .leading, spacing: 4)`:
    - `Text("Remit has held this action")` — `.headline`, white
    - `Text("REASON: \(reason)")` — `.subheadline.monospaced()`, `.secondary`

### Sections (each one `InfoSection`)

| Caption | Body |
|---|---|
| ACTION | `chevron.left.forward.slash` + cwd (monospaced, `.secondary`); below, `Text("\(toolName): \(args_oneline)").font(.headline)`. |
| ACTION ARGS | Full prettified args inside a `ScrollView`, monospaced, max height 140pt. No truncation. |
| AGENT SCOPE | `lock.shield` (`.secondary` tint — scope is informational, not warning) + `Text(scope).font(.headline)`. |

### Picker

- Caption: `CHOOSE AN ACTION:`.
- Surface containing four `PickerRow`s:
  - **Deny** — `nosign`. Default selection. Indigo fill, white text, `ENTER` keycap badge on the right.
  - **Allow** — `checkmark.circle`.
  - **Update scope** — `key`.
  - **Disable session** — `rectangle.portrait.and.arrow.right`, label in system red (destructive).
- Selection driven by an `ObservableObject` coordinator (`@Published var selectedIndex`); the `NSHostingView` subclass (see Keyboard and focus) maps arrow keys to coordinator mutations. Return commits via the coordinator; Escape cancels.

### Footer

- Hint capsules: `↑↓` choose · `RETURN` confirm · `ESC` cancel (each key in a `KeycapHint`).
- No trailing button; the picker carries the action. Esc and title-bar X cancel.

## Update scope dialog

Same chrome as the choose dialog — `TitleBar`, background tokens, outer padding, `InfoSection` caption-outside-surface pattern, `DialogFooter` with `KeycapHint` capsules. No warning banner (no Red verdict to surface).

### Lead copy

- Above the first section, in `.secondary`: `"Set the scope for this session."` when `current.isNil`, `"Update the scope for this session."` otherwise.

### Sections (each `InfoSection`)

| Caption | Body |
|---|---|
| CURRENT DIRECTORY | `chevron.left.forward.slash` + cwd, monospaced (same pattern as choose dialog's ACTION row). |
| SCOPE | `key` icon + lead label, then `TextEditor` inside a surface — monospaced, transparent background, min 80pt, grows then scrolls past 140pt. Pre-filled with `current` when present. |

### Footer

Hint capsules: `RETURN` save · `OPT+RETURN` newline · `ESC` cancel. Trailing button: `Save` (default). Esc and the title-bar X cancel.

Return saves; Opt+Return inserts a newline. Focus starts in the editor.

## Confirm disable dialog

Same chrome — `TitleBar`, background tokens, outer padding, `DialogFooter`. No sections; the decision is the whole content.

**One exception to the no-footer-Cancel rule.** Cancel is the default; Disable sits beside it. Reaching Disable takes an extra `→` then `RETURN` — the destructive action is deliberately two-handed.

- `VStack(alignment: .leading, spacing: 8)`:
  - `Text("Disable Remit for the rest of this session?")` — `.headline`, white
  - `Text("Classification will be skipped until the Claude Code session ends.")` — `.secondary`

Footer: `←→` choose · `RETURN` confirm · `ESC` cancel. Trailing buttons: `Cancel` (default), `Disable` (red destructive). Esc and title-bar X cancel.

Safety layers: (1) reaching this dialog requires Down × 3 + Return through the choose picker; (2) Cancel is the default. No input-arming gate: this dialog is never the cold-first popup, so there's nothing to race against.

## Keyboard and focus

Keyboard handling bridges through a thin `NSHostingView` subclass. Works on the current macOS 13 deployment target without bumping to macOS 14 for `.onKeyPress`.

- **Coordinators** (`ObservableObject`, `@MainActor`) per dialog — type names: `ChooseCoordinator`, `UpdateScopeCoordinator`, `ConfirmDisableCoordinator`. These replace the existing `ChooseController` / `UpdateScopeController` / `ConfirmDisableController` AppKit classes; the rename signals the framework shift (AppKit `NSObject` controllers → SwiftUI `ObservableObject` coordinators). Each holds:
  - `@Published var inputArmed: Bool`
  - A selection field (`selectedIndex: Int` for choose; `selectedButton: ConfirmDisableButton` enum for confirm-disable; n/a for update_scope)
  - `commit()`, `cancel()`, `handleArrow(_ direction: ArrowDirection)` methods
  - A `response` property the wrapper reads after `runModal` returns
- **`KeyHandlingHostingView<Content: View>`**: `NSHostingView` subclass overriding `keyDown(with:)`. Holds three closures set at construction:
  - `var onArrow: (ArrowDirection) -> Void`
  - `var onReturn: () -> Void`
  - `var onEsc: () -> Void`

  Maps `↑/↓/←/→` → `onArrow`, `Return` → `onReturn`, `Esc` → `onEsc`. Anything else falls through to `super` so `TextEditor` keeps its normal behaviour. The hosting view stays dumb about dialog semantics; each wrapper hooks the closures to its coordinator's methods. ~30 lines in `DialogChrome.swift`.
- **First responder** set to the hosting view via `makeFirstResponder(hostingView)`. Inside SwiftUI, `@FocusState` + `.focused(...)` is used only for the `TextEditor` in `update_scope` (so the caret lands inside it). Other dialogs route through the coordinator rather than per-view focus.
- **Initial coordinator state**:
  - Choose: `selectedIndex = 0` (Deny).
  - Update scope: focus drops into the text editor; no picker.
  - Confirm disable: `selectedButton = .cancel`.
- **Editing shortcuts**: Main menu install (Edit + Window submenus) stays in `Dialog.ensureAppKitReady()` so `TextEditor` gets Cmd+V/C/X/A/Z/Cmd+W via the responder chain.
- **Esc / title-bar X**: both route through `CloseAsCancelDelegate`. Esc path: hosting-view's `onEsc` closure calls coordinator `cancel()`, which sets the response and `NSApp.stopModal`s.

## Input arming (terminal-race guard)

A Red verdict interrupts the terminal mid-keystroke. Common race: user hits Return as the dialog appears, dismissing it unread.

- Coordinator holds `inputArmed = false`; a `.task` flips it true after 1s.
- `commit()` and editor typing early-return when `!inputArmed`. `cancel()` is **not** gated — Esc, title-bar X, and the Cancel button remain live throughout. Arming exists to absorb a careless terminal Enter, not to block deliberate escape gestures.
- No visible indicator. Within 1s the dialog feels instantly responsive; before that, commit-shaped keystrokes drop silently.
- Applied to **choose** and **update_scope** (the cold-popup paths).
- **Not applied to confirm_disable** — it only appears after explicit picker navigation, so the cold-race doesn't exist. Cancel-as-default already guards an accidental Return.

## Wire change

- Drop head+tail truncation in `crates/swift-prompter`. The ACTION ARGS box renders full prettified content; the 64 KiB stdin cap remains the upstream guard. `prettify_args` stays.
- Rename the wire field `args_pretty` → `args_pretty` on both sides in the same release. The previous name encoded a truncation behaviour we no longer apply; per CDR-0005 (current-state-only style), the name must reflect what it now is.
- ACTION row needs a short oneline form (first ~80 chars) alongside the full args block. Compute in Swift from the same `args_pretty` field — one source of truth.
- No `protocol` version bump (both sides update together; no mixed-version compatibility surface).

## Out of scope

- Pure `@main App` + `WindowGroup` (rejected — see Decision).
- Light-mode token tuning.
- Localisation of keyboard hints.
- Animations.
- Resizing — fixed window width; only ACTION ARGS and SCOPE editor scroll.
- Theming.
- Reason copy sanitisation (the `classifier unavailable: <URL>` leak). Separate concern.
- Cancel-vs-Deny picker redundancy. Pre-existing; not this rebuild.

## Test strategy

Pattern matches TN-2026-011 / TN-2026-012: Rust unit tests for adapter changes, Swift Codable round-trips, manual smoke for visuals.

### Rust (`crates/swift-prompter`)

- Drop existing head+tail truncation tests; keep `prettify_args` tests as-is.
- New test: `args_pretty` for a 10KB input passes through prettified but un-truncated (asserts the upstream cap is the only guard).
- Existing no-leak / shape-mismatch / non-zero-exit tests remain unchanged — adapter behaviour around error paths is untouched.

### Swift (`swift/Tests/RemitPrompterTests`)

- Existing `Wire` Codable round-trips remain unchanged (wire shape unchanged).
- New: coordinator unit tests for each dialog — arrow keys clamp at bounds; commit/cancel set response correctly; `inputArmed` guard drops keys while false.
- SwiftUI view snapshot testing not pursued in v1 (toolchain overhead, fragile under macOS version drift). Visual fidelity covered by the manual smoke checklist.

### Manual smoke (per dialog, dark mode)

- Cold appearance: dialog grabs focus from the terminal without dock-icon flash.
- Title-bar wordmark centred; traffic lights and close X present.
- All SF Symbol icons render; tints match spec.
- **Choose**: every row in the picker keyboard-navigable; Deny pre-selected; ENTER badge on selected row; Disable session row text reads system red; long `args_pretty` fills the ACTION ARGS box and scrolls.
- **Update scope**: editor receives focus; Cmd+V/C/X/A/Z/Cmd+W work; Save commits; Esc cancels.
- **Confirm disable**: focus on Cancel; `→` moves to Disable; Return on Cancel cancels; Return on Disable commits.
- **Input arming**: hold Enter in a terminal then trigger a Red — first 1s of Enter presses must not dismiss the dialog. Confirm choose and update_scope are armed; confirm_disable is not (immediate).

## Implementation staging

Single PR, ordered so each step is independently verifiable:

1. **Chrome scaffold** — add `DialogChrome.swift` with `DialogTokens`, `TitleBar`, `KeycapHint`, empty `InfoSection`, `KeyHandlingHostingView` shell. Add SwiftUI token extensions to `Branding.swift`. Build green; no behavioural change yet.
2. **Wire `Dialog.swift`** — keep `makeWindow` + activation; delete `ArrowKeyWindow`, `makeLabel`, `makeFieldLabel`, `makeButton`. Add a hosted-runModal helper taking a SwiftUI `View` builder + coordinator. Build green.
3. **Choose dialog (vertical slice)** — rebuild end-to-end against the new chrome. First validation of the hosted pattern. Manual smoke.
4. **Update scope dialog** — rebuild reusing chrome. Manual smoke for editing shortcuts.
5. **Confirm disable dialog** — rebuild. Manual smoke for two-button `←→` pattern.
6. **Rust adapter** — drop `truncate_head_tail` from `crates/swift-prompter` args pipeline + matching tests.
7. **Full manual smoke** across all three dialogs with realistic payloads (real `args_pretty` content from the existing log).

All three dialogs land together; staggered landings would leave visible inconsistency between them.

## Risks

- **`NSHostingView` first-responder + `@FocusState`.** Initial focus into a SwiftUI `TextEditor` inside `NSHostingView` is well-trodden but has had macOS-version-specific quirks (most resolved by macOS 13). Mitigation: vertical-slice choose first; expose issues before all three are rebuilt.
- **macOS 13 deployment target.** Spec avoids `.onKeyPress` (macOS 14+) via the `keyDown` bridge. A future macOS-14 bump can collapse the coordinator pattern into `.onKeyPress` calls; defer.
- **Two title-bar accessories** (centred wordmark + right-edge close X). `NSTitlebarAccessoryViewController` supports multiple via explicit `layoutAttribute`; risk is alignment polish, not correctness.
- **Input arming as a silent guard.** A regression that leaves the gate permanently disarmed would be invisible. Mitigation: coordinator unit test asserting `inputArmed` flips true within ~1.2s of view init; manual smoke step explicitly testing the race.
- **Materials + custom backgrounds.** `Material.ultraThin` over custom navy may render differently across hardware. Mitigation: eyeball on the dev machine; light-mode out of scope.
- **Cmd+V in `TextEditor`** depends on the existing main-menu install routing correctly. Mitigation: explicit manual smoke step. (If broken, fix path is the menu install in `ensureAppKitReady`, same as TN-2026-012.)

## Files touched

- New: `swift/Sources/RemitPrompter/DialogChrome.swift`.
- Modify: `swift/Sources/RemitPrompter/Dialog.swift` (drop field/button builders + `ArrowKeyWindow`; keep activation/menu/window).
- Modify: `swift/Sources/RemitPrompter/Branding.swift` (SwiftUI tokens).
- Modify: `swift/Sources/RemitPrompter/Wire.swift` (rename `args_preview` → `args_pretty` in `ChooseRequest`).
- Rewrite: `swift/Sources/RemitPrompter/ChooseDialog.swift`, `UpdateScopeDialog.swift`, `ConfirmDisableDialog.swift`.
- Modify: `crates/swift-prompter/src/lib.rs` (drop `truncate_head_tail` from the args pipeline; rename wire field `args_preview` → `args_pretty`).
- Modify: `crates/swift-prompter/src/tests.rs` (drop truncation tests; update field name).
- Modify: `docs/tn/README.md` (title + filename).
- Update: issue #54 title/body to reflect rebuild scope.
