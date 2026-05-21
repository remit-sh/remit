import AppKit
import Foundation
import SwiftUI

/// AppKit shell shared by all three dialogs.
///
/// Owns the modal lifecycle (`runModal` blocks until a coordinator calls
/// `NSApp.stopModal()`), the main menu (so `TextEditor` gets the standard
/// editing shortcuts), and the window + title-bar accessory plumbing. The
/// dialog content is SwiftUI inside `NSHostingView` — see `DialogChrome.swift`
/// for the bridge.
enum Dialog {
    /// Configure the running process as a UI-eligible accessory app, then
    /// hand focus to our window. The hook spawns us as a CLI tool with no
    /// activation policy; without this call AppKit refuses to render windows.
    /// Returns false when WindowServer isn't available (SSH session, etc.),
    /// so `main` can exit 3 instead of trapping inside AppKit.
    @MainActor
    static func ensureAppKitReady() -> Bool {
        // CGSessionCopyCurrentDictionary returns nil when there's no GUI
        // session; AppKit on the same process would later trap.
        guard CGSessionCopyCurrentDictionary() != nil else { return false }
        // Forces NSApplication initialisation — without this NSApp is nil
        // because we have no `@NSApplicationMain`/`NSApplicationMain()` call.
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        // Install a minimal main menu so Cocoa wires Cmd+V/C/X/A/Z/Shift+Z
        // and Cmd+W via the Edit/Window submenus' key equivalents. Accessory
        // policy hides the menu bar; only the shortcuts are observable.
        // Without this, NSTextView's paste:/cut:/copy:/selectAll:/undo:/redo:
        // responders never fire because Cocoa has nowhere to look up the
        // shortcut.
        app.mainMenu = makeMainMenu()
        app.activate(ignoringOtherApps: true)
        return true
    }

    /// Minimal `NSApp.mainMenu` providing only the standard editing key
    /// equivalents needed by the scope text input. `target = nil` routes each
    /// action through the responder chain to the focused NSTextView.
    @MainActor
    private static func makeMainMenu() -> NSMenu {
        let main = NSMenu()

        let editItem = NSMenuItem()
        editItem.submenu = NSMenu(title: "Edit")
        for (title, selector, key) in [
            ("Undo", #selector(UndoManager.undo), "z"),
            ("Redo", #selector(UndoManager.redo), "Z"),  // Cmd+Shift+Z
            ("Cut", #selector(NSText.cut(_:)), "x"),
            ("Copy", #selector(NSText.copy(_:)), "c"),
            ("Paste", #selector(NSText.paste(_:)), "v"),
            ("Delete", #selector(NSText.delete(_:)), ""),
            ("Select All", #selector(NSText.selectAll(_:)), "a"),
        ] {
            let item = NSMenuItem(title: title, action: selector, keyEquivalent: key)
            editItem.submenu?.addItem(item)
        }
        main.addItem(editItem)

        let windowItem = NSMenuItem()
        windowItem.submenu = NSMenu(title: "Window")
        let close = NSMenuItem(
            title: "Close",
            action: #selector(NSWindow.performClose(_:)),
            keyEquivalent: "w"
        )
        windowItem.submenu?.addItem(close)
        main.addItem(windowItem)

        return main
    }

    /// Build a window pre-styled with the Remit chrome. Width is fixed; height
    /// is driven by the hosted SwiftUI content's intrinsic size.
    @MainActor
    static func makeWindow() -> NSWindow {
        // Height is a placeholder; the hosted SwiftUI content's intrinsic
        // size drives the final window height after layout.
        let rect = NSRect(x: 0, y: 0, width: Branding.windowWidth, height: 200)
        let window = NSWindow(
            contentRect: rect,
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "Remit"
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isMovableByWindowBackground = true
        window.level = .modalPanel
        return window
    }
}

// MARK: - AppKit bridge: KeyHandlingHostingView

enum ArrowDirection {
    case up, down, left, right
}

/// `NSHostingView` subclass that maps arrow / Return / Esc keys to
/// caller-supplied closures. Anything else falls through to `super`, so a
/// focused `TextEditor` keeps its normal text-editing behaviour.
final class KeyHandlingHostingView<Content: View>: NSHostingView<Content> {
    var onArrow: ((ArrowDirection) -> Void)?
    var onReturn: (() -> Void)?
    var onEsc: (() -> Void)?

    override var acceptsFirstResponder: Bool { true }

    @MainActor required init(rootView: Content) {
        super.init(rootView: rootView)
    }

    @MainActor required dynamic init?(coder aDecoder: NSCoder) {
        super.init(coder: aDecoder)
    }

    override func keyDown(with event: NSEvent) {
        guard
            let chars = event.charactersIgnoringModifiers,
            let scalar = chars.unicodeScalars.first
        else {
            super.keyDown(with: event)
            return
        }
        switch Int(scalar.value) {
        case 0xF700: onArrow?(.up)
        case 0xF701: onArrow?(.down)
        case 0xF702: onArrow?(.left)
        case 0xF703: onArrow?(.right)
        case 0x000D, 0x0003: onReturn?()  // Return / numpad Enter
        case 0x001B: onEsc?()
        default: super.keyDown(with: event)
        }
    }
}

/// Window-close-button handler. Routes the close gesture (red traffic light or
/// our custom title-bar X) through the supplied closure so the dialog
/// coordinator can record a `cancelled` response before the modal stops.
@MainActor
final class CloseAsCancelDelegate: NSObject, NSWindowDelegate {
    let onClose: () -> Void

    init(onClose: @escaping () -> Void) {
        self.onClose = onClose
        super.init()
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        onClose()
        return false
    }
}

// MARK: - Hosted SwiftUI runModal

extension Dialog {
    /// Run a SwiftUI dialog inside the modal NSWindow shell.
    ///
    /// The coordinator is owned by the caller — pass closures that route key
    /// events into it. Title-bar accessories (centred wordmark, right-edge
    /// close X) are installed automatically; window-close gestures call the
    /// same `onEsc` closure so cancellation has a single code path.
    @MainActor
    static func runHosted<RootView: View>(
        rootView: RootView,
        onArrow: @escaping (ArrowDirection) -> Void,
        onReturn: @escaping () -> Void,
        onEsc: @escaping () -> Void
    ) {
        let window = makeWindow()
        let hostingView = KeyHandlingHostingView(rootView: rootView)
        hostingView.onArrow = onArrow
        hostingView.onReturn = onReturn
        hostingView.onEsc = onEsc
        window.contentView = hostingView

        installTitleBarAccessories(on: window, onClose: onEsc)

        let delegate = CloseAsCancelDelegate(onClose: onEsc)
        window.delegate = delegate

        window.makeKeyAndOrderFront(nil)
        window.center()
        window.makeFirstResponder(hostingView)

        // Surface signal: tells the Rust adapter the dialog is up so it can
        // drop its surface-phase deadline and wait indefinitely for the user.
        // Must be written post makeKeyAndOrderFront, pre runModal. See
        // TN-2026-014.
        FileHandle.standardError.write(Data("READY\n".utf8))

        _ = withExtendedLifetime(delegate) {
            NSApp.runModal(for: window)
        }
    }

    /// Install the centred wordmark and the right-edge close X as
    /// `NSTitlebarAccessoryViewController` instances. Both routed through
    /// closures, no AppKit subclassing required.
    @MainActor
    private static func installTitleBarAccessories(
        on window: NSWindow,
        onClose: @escaping () -> Void
    ) {
        // Centred wordmark: full-width accessory with internal Spacers so the
        // image sits in the middle of the title bar regardless of the
        // accessory's layout-attribute slot.
        let wordmarkVC = NSTitlebarAccessoryViewController()
        let wordmarkView = NSHostingView(
            rootView: HStack {
                Spacer(minLength: 0)
                TitleBarWordmark()
                Spacer(minLength: 0)
            }
            .frame(maxWidth: .infinity)
        )
        wordmarkView.frame = NSRect(x: 0, y: 0, width: Branding.windowWidth, height: 28)
        wordmarkVC.view = wordmarkView
        wordmarkVC.layoutAttribute = .right
        window.addTitlebarAccessoryViewController(wordmarkVC)

        // Right-edge close X. Sits inside the title bar; same code path as Esc.
        let closeVC = NSTitlebarAccessoryViewController()
        let closeView = NSHostingView(rootView: TitleBarClose(action: onClose))
        closeView.frame = NSRect(x: 0, y: 0, width: 32, height: 28)
        closeVC.view = closeView
        closeVC.layoutAttribute = .right
        window.addTitlebarAccessoryViewController(closeVC)
    }
}
