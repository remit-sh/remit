import AppKit
import Foundation

/// Glue helpers shared by all three dialogs.
///
/// AppKit modal pattern: build window + controller, call `runModal(for:)` which
/// blocks until a button calls `stopModal()`. Window-close via the title bar's
/// red dot is also a stop, mapped to a `cancelled` response.
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
        app.activate(ignoringOtherApps: true)
        return true
    }

    /// Build a window pre-styled with the Remit chrome. Width is fixed; height
    /// is driven by Auto Layout on the content. Callers add views and wire
    /// constraints; we don't fix a height here.
    @MainActor
    static func makeWindow() -> ArrowKeyWindow {
        let rect = NSRect(x: 0, y: 0, width: Branding.windowWidth, height: 200)
        let window = ArrowKeyWindow(
            contentRect: rect,
            styleMask: [.titled, .closable],
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

    /// Standard text label (single line, no edits, no background). Body
    /// dialogs use these for cwd / Reason / Scope rows.
    @MainActor
    static func makeLabel(_ text: String, font: NSFont? = nil, wraps: Bool = true) -> NSTextField {
        let label = NSTextField(labelWithString: text)
        label.font = font ?? NSFont.systemFont(ofSize: NSFont.systemFontSize)
        label.isSelectable = false
        label.usesSingleLineMode = !wraps
        label.lineBreakMode = wraps ? .byWordWrapping : .byTruncatingTail
        if wraps {
            label.preferredMaxLayoutWidth = Branding.windowWidth - 2 * Branding.edgeInset
        }
        return label
    }

    /// `**Label:** value` body row using attributed text. The bold prefix lets
    /// the eye separate field name from content without forcing a two-column
    /// layout (which fights with long wrapping values).
    @MainActor
    static func makeFieldLabel(label: String, value: String) -> NSTextField {
        let body = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        let bold = NSFont.boldSystemFont(ofSize: NSFont.systemFontSize)
        let attrs = NSMutableAttributedString()
        attrs.append(NSAttributedString(string: "\(label) ", attributes: [.font: bold]))
        attrs.append(NSAttributedString(string: value, attributes: [.font: body]))
        let field = NSTextField(labelWithAttributedString: attrs)
        field.isSelectable = false
        field.usesSingleLineMode = false
        field.lineBreakMode = .byWordWrapping
        field.preferredMaxLayoutWidth = Branding.windowWidth - 2 * Branding.edgeInset
        return field
    }

    /// Wordmark image view (or empty placeholder if asset missing).
    @MainActor
    static func makeWordmark() -> NSView {
        let view = NSImageView()
        view.image = Branding.wordmark()
        view.imageScaling = .scaleProportionallyDown
        view.imageAlignment = .alignLeft
        view.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            view.widthAnchor.constraint(equalToConstant: Branding.wordmarkSize.width),
            view.heightAnchor.constraint(equalToConstant: Branding.wordmarkSize.height),
        ])
        return view
    }

    /// Push-button factory. `keyEquivalent` accepts "\r" for default Return
    /// activation or a single letter for `<letter>`-press shortcuts (no Cmd).
    @MainActor
    static func makeButton(
        title: String,
        keyEquivalent: String = "",
        target: AnyObject,
        action: Selector
    ) -> NSButton {
        let button = NSButton(title: title, target: target, action: action)
        button.bezelStyle = .rounded
        if !keyEquivalent.isEmpty {
            button.keyEquivalent = keyEquivalent
            if keyEquivalent != "\r" {
                // Letter shortcut: no Cmd needed, matches NSAlert convention.
                button.keyEquivalentModifierMask = []
            }
        }
        return button
    }

    /// Assemble a content stack (above) + button-bar (below) inside the given
    /// window. Buttons are passed left-to-right; the rightmost is the default.
    /// Wires arrow-key navigation through `ArrowKeyWindow`.
    @MainActor
    static func assemble(
        window: ArrowKeyWindow,
        delegate: NSWindowDelegate,
        contentViews: [NSView],
        customSpacings: [(after: NSView, spacing: CGFloat)] = [],
        buttons: [NSButton],
        defaultFocus: NSResponder? = nil
    ) {
        window.delegate = delegate
        let content = window.contentView!

        let topStack = NSStackView(views: contentViews)
        topStack.orientation = .vertical
        topStack.alignment = .leading
        topStack.spacing = Branding.stackSpacing
        for spacing in customSpacings {
            topStack.setCustomSpacing(spacing.spacing, after: spacing.after)
        }
        topStack.edgeInsets = NSEdgeInsets(
            top: Branding.edgeInset,
            left: Branding.edgeInset,
            bottom: Branding.edgeInset,
            right: Branding.edgeInset
        )
        topStack.translatesAutoresizingMaskIntoConstraints = false

        let separator = NSBox()
        separator.boxType = .separator
        separator.translatesAutoresizingMaskIntoConstraints = false

        // Buttons render left-to-right in the order passed; the rightmost is
        // expected to be the default (macOS HIG).
        let buttonRow = NSStackView(views: buttons)
        buttonRow.orientation = .horizontal
        buttonRow.alignment = .centerY
        buttonRow.spacing = Branding.buttonSpacing
        buttonRow.translatesAutoresizingMaskIntoConstraints = false

        let bottomBar = NSView()
        bottomBar.translatesAutoresizingMaskIntoConstraints = false
        bottomBar.addSubview(buttonRow)

        content.addSubview(topStack)
        content.addSubview(separator)
        content.addSubview(bottomBar)

        NSLayoutConstraint.activate([
            content.widthAnchor.constraint(equalToConstant: Branding.windowWidth),

            topStack.topAnchor.constraint(equalTo: content.topAnchor),
            topStack.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            topStack.trailingAnchor.constraint(equalTo: content.trailingAnchor),

            separator.topAnchor.constraint(equalTo: topStack.bottomAnchor),
            separator.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            separator.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            separator.heightAnchor.constraint(equalToConstant: 1),

            bottomBar.topAnchor.constraint(equalTo: separator.bottomAnchor),
            bottomBar.leadingAnchor.constraint(equalTo: content.leadingAnchor),
            bottomBar.trailingAnchor.constraint(equalTo: content.trailingAnchor),
            bottomBar.bottomAnchor.constraint(equalTo: content.bottomAnchor),
            bottomBar.heightAnchor.constraint(equalToConstant: Branding.bottomBarHeight),

            // Right-align the button row; native macOS sheet pattern.
            buttonRow.trailingAnchor.constraint(equalTo: bottomBar.trailingAnchor, constant: -Branding.edgeInset),
            buttonRow.centerYAnchor.constraint(equalTo: bottomBar.centerYAnchor),
            buttonRow.leadingAnchor.constraint(greaterThanOrEqualTo: bottomBar.leadingAnchor, constant: Branding.edgeInset),
        ])

        // Left/right arrows step focus through the buttons. Default button is
        // last (rightmost in `buttons`); when no other field has focus, arrows
        // navigate the bar.
        window.arrowNavigableButtons = buttons
        window.makeKeyAndOrderFront(nil)
        window.center()
        if let r = defaultFocus {
            window.makeFirstResponder(r)
        } else if let last = buttons.last {
            window.makeFirstResponder(last)
        }
    }
}

/// `NSWindow` subclass that maps ←/→ to focus traversal through the bottom
/// button bar — matches the user's mental model from elsewhere in macOS where
/// arrow keys move between Cancel/OK in a sheet.
///
/// Arrow navigation is suppressed when an editable text view holds focus, so
/// in `update_scope` the arrows still move the caret inside the scope field.
@MainActor
final class ArrowKeyWindow: NSWindow {
    var arrowNavigableButtons: [NSButton] = []

    override func keyDown(with event: NSEvent) {
        // Suppress when a control owns its own arrow handling: text editors
        // (caret movement) and table views (row selection). Otherwise we'd
        // steal arrow keys mid-typing or mid-list-navigation.
        if firstResponder is NSText || firstResponder is NSTableView {
            super.keyDown(with: event)
            return
        }
        let chars = event.charactersIgnoringModifiers ?? ""
        let scalar = chars.unicodeScalars.first.map { Int($0.value) } ?? 0
        let leftArrow = 0xF702
        let rightArrow = 0xF703
        guard scalar == leftArrow || scalar == rightArrow else {
            super.keyDown(with: event)
            return
        }
        // Only meaningful with ≥2 buttons; one-button bar has nothing to cycle.
        guard arrowNavigableButtons.count > 1 else {
            super.keyDown(with: event)
            return
        }
        let current = arrowNavigableButtons.firstIndex { $0 === firstResponder }
        let count = arrowNavigableButtons.count
        let next: Int
        if let current {
            let step = (scalar == rightArrow) ? 1 : -1
            next = (current + step + count) % count
        } else {
            next = (scalar == rightArrow) ? 0 : count - 1
        }
        makeFirstResponder(arrowNavigableButtons[next])
    }
}

/// Window-close-button handler. Treats the user closing the window via the
/// title bar's red dot as a cancellation.
@MainActor
final class CloseAsCancelDelegate: NSObject, NSWindowDelegate {
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        NSApp.stopModal(withCode: .cancel)
        return false
    }
}
