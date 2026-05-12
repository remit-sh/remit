import AppKit
import Foundation

/// "Set/Update the scope" — text input dialog with multi-line NSTextView.
///
/// Cmd-Return saves; Escape (handled by window close) cancels.
@MainActor
final class UpdateScopeController: NSObject, NSTextViewDelegate {
    let textView: NSTextView
    private(set) var response: UpdateScopeResponse = .cancelled

    init(textView: NSTextView) {
        self.textView = textView
    }

    @objc func save() {
        let raw = textView.string
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            response = .cancelled
        } else {
            response = .scope(trimmed)
        }
        NSApp.stopModal()
    }

    @objc func cancel() {
        response = .cancelled
        NSApp.stopModal()
    }

    /// Plain Return saves; Opt+Return inserts a newline (AppKit binds that to
    /// `insertNewlineIgnoringFieldEditor:` which we don't intercept).
    func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(NSResponder.insertNewline(_:)) {
            save()
            return true
        }
        return false
    }
}

enum UpdateScopeDialog {
    @MainActor
    static func run(_ req: UpdateScopeRequest) -> UpdateScopeResponse {
        let isFirstTime = req.current == nil
        let lead = isFirstTime
            ? "Set the scope for this session."
            : "Update the scope for this session."

        let wordmark = Dialog.makeWordmark()
        let cwd = Dialog.makeLabel(
            Branding.displayDir(req.working_dir),
            font: NSFont.userFixedPitchFont(ofSize: NSFont.smallSystemFontSize),
            wraps: false
        )
        cwd.textColor = .secondaryLabelColor

        let body = Dialog.makeLabel(lead)

        // Bordered scroll view with an editable NSTextView. Word wrap on; text
        // view tracks the scroll view's width so wrapping is visible without
        // horizontal scrolling.
        let scroll = NSScrollView()
        scroll.borderType = .bezelBorder
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
        scroll.autohidesScrollers = true
        scroll.translatesAutoresizingMaskIntoConstraints = false

        let textView = NSTextView()
        textView.isEditable = true
        textView.isRichText = false
        textView.font = NSFont.userFixedPitchFont(ofSize: 13)
        textView.string = req.current ?? ""
        textView.allowsUndo = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.minSize = NSSize(width: 0, height: 80)
        textView.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainerInset = NSSize(width: 8, height: 8)
        textView.textContainer?.containerSize = NSSize(
            width: Branding.windowWidth - 2 * Branding.edgeInset - 24,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.textContainer?.widthTracksTextView = true

        scroll.documentView = textView

        let controller = UpdateScopeController(textView: textView)
        textView.delegate = controller
        let delegate = CloseAsCancelDelegate()

        let cancelButton = Dialog.makeButton(
            title: "Cancel",
            keyEquivalent: "\u{1B}", // Escape
            target: controller,
            action: #selector(UpdateScopeController.cancel)
        )
        // `keyEquivalent: "\r"` is for the visual default-button highlight,
        // not for the Return action: the NSTextViewDelegate intercepts Return
        // before AppKit dispatches to the default button. Leave it in for the
        // pulse animation that tells the user which button Return targets.
        let saveButton = Dialog.makeButton(
            title: "Save",
            keyEquivalent: "\r",
            target: controller,
            action: #selector(UpdateScopeController.save)
        )

        let window = Dialog.makeWindow()

        // Scroll view needs an explicit height + width to fill content width.
        scroll.heightAnchor.constraint(equalToConstant: 80).isActive = true
        scroll.widthAnchor.constraint(
            equalToConstant: Branding.windowWidth - 2 * Branding.edgeInset
        ).isActive = true

        Dialog.assemble(
            window: window,
            delegate: delegate,
            contentViews: [wordmark, cwd, body, scroll],
            customSpacings: [(after: wordmark, spacing: Branding.headerSpacing)],
            buttons: [cancelButton, saveButton],
            defaultFocus: textView
        )

        _ = withExtendedLifetime((controller, delegate)) {
            NSApp.runModal(for: window)
        }
        return controller.response
    }
}
