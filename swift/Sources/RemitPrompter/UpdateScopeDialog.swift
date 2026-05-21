import AppKit
import SwiftUI

// "Set/Update the scope" — multi-line text editor backed by NSTextView
// (wrapped via NSViewRepresentable). Plain Return saves; Opt+Return inserts
// a newline. The 1-second input arming gates typing too (via the text view
// delegate's shouldChangeTextIn).

// MARK: - Coordinator

@MainActor
final class UpdateScopeCoordinator: ObservableObject {
    @Published var inputArmed: Bool = false
    let initialText: String

    /// Live text from the editor. Mutated by the text view's delegate on
    /// every keystroke, read by `commit()` at save time. Not `@Published`
    /// because SwiftUI doesn't need to re-render on every keystroke.
    var currentText: String

    private(set) var response: UpdateScopeResponse = .cancelled

    init(initialText: String) {
        self.initialText = initialText
        self.currentText = initialText
    }

    func arm() {
        scheduleInputArming { [weak self] in self?.inputArmed = true }
    }

    func commit() {
        guard inputArmed else { return }
        let trimmed = currentText.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            response = .cancelled
        } else {
            response = .scope(trimmed)
        }
        NSApp.stopModal()
    }

    func cancel() {
        // Cancel is not gated by `inputArmed`: arming exists to absorb a
        // terminal Enter that hits the dialog before the user reads it,
        // not to block deliberate Esc/close-X escape gestures.
        response = .cancelled
        NSApp.stopModal()
    }

    /// Arrows in this dialog move the editor's caret — handled by NSTextView
    /// before keys reach the hosting view. If a key does reach us, the editor
    /// isn't focused; safe to ignore.
    func handleArrow(_ direction: ArrowDirection) {}
}

// MARK: - ScopeEditor (NSTextView wrapper)

/// NSTextView subclass that forwards Esc to a closure. By default NSTextView
/// swallows Esc as `cancelOperation:`, so the dialog's hosting view never sees
/// the key — we'd lose the cancel path while the editor is focused.
final class EscapeForwardingTextView: NSTextView {
    var onEsc: (() -> Void)?

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 {  // Esc
            onEsc?()
            return
        }
        super.keyDown(with: event)
    }
}

struct ScopeEditor: NSViewRepresentable {
    let coordinator: UpdateScopeCoordinator

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.borderType = .noBorder
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
        scroll.autohidesScrollers = true
        scroll.drawsBackground = false

        let textView = EscapeForwardingTextView()
        textView.onEsc = { [weak coordinator] in coordinator?.cancel() }
        textView.delegate = context.coordinator
        textView.isEditable = true
        textView.isRichText = false
        textView.font = NSFont.userFixedPitchFont(ofSize: 13)
        textView.textColor = .white
        textView.insertionPointColor = .white
        textView.backgroundColor = .clear
        textView.drawsBackground = false
        textView.allowsUndo = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        textView.isAutomaticSpellingCorrectionEnabled = false
        textView.textContainerInset = NSSize(width: 8, height: 8)

        // Vertically-resizable text view that tracks the scroll view's width.
        // Without these, the textView ignores its container and the cursor
        // ends up centred in the visible bounds rather than at top-leading.
        textView.minSize = NSSize(width: 0, height: 0)
        textView.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.containerSize = NSSize(
            width: Branding.windowWidth,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.textContainer?.widthTracksTextView = true

        // Set content AFTER resizable config so layout uses the right bounds.
        // Use an attributed string so the foreground colour applies to the
        // pre-loaded text too (textColor only applies to newly-typed runs).
        let attrs: [NSAttributedString.Key: Any] = [
            .foregroundColor: NSColor.white,
            .font: NSFont.userFixedPitchFont(ofSize: 13) as Any,
        ]
        textView.textStorage?.setAttributedString(
            NSAttributedString(string: coordinator.initialText, attributes: attrs)
        )
        // Typing attributes for any later input.
        textView.typingAttributes = attrs

        // Grab first responder once attached to a window so the user can
        // start typing (and arrow keys move the caret rather than triggering
        // the hosting view's onArrow handler).
        DispatchQueue.main.async { [weak textView] in
            textView?.window?.makeFirstResponder(textView)
        }

        scroll.documentView = textView
        return scroll
    }

    func updateNSView(_ nsView: NSScrollView, context: Context) {}

    func makeCoordinator() -> Bridge {
        Bridge(dialog: coordinator)
    }

    /// SwiftUI's `makeCoordinator()` convention names the bridge `Coordinator`,
    /// but that collides with `UpdateScopeCoordinator` in this file. Call it
    /// `Bridge` and bind via a typealias so SwiftUI still resolves the type.
    typealias Coordinator = Bridge

    @MainActor
    final class Bridge: NSObject, NSTextViewDelegate {
        let dialog: UpdateScopeCoordinator

        init(dialog: UpdateScopeCoordinator) {
            self.dialog = dialog
        }

        // Gate typing on the input-arming flag. Keystrokes that arrive before
        // the 1-second timer flips `inputArmed` are silently dropped.
        func textShouldBeginEditing(_ textObject: NSText) -> Bool {
            dialog.inputArmed
        }

        func textView(
            _ textView: NSTextView,
            shouldChangeTextIn affectedCharRange: NSRange,
            replacementString: String?
        ) -> Bool {
            dialog.inputArmed
        }

        func textDidChange(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            dialog.currentText = textView.string
        }

        /// Plain Return saves; Opt+Return inserts a newline (AppKit binds that
        /// to `insertNewlineIgnoringFieldEditor:` which we don't intercept).
        func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
            if commandSelector == #selector(NSResponder.insertNewline(_:)) {
                dialog.commit()
                return true
            }
            return false
        }
    }
}

// MARK: - View

struct UpdateScopeView: View {
    let req: UpdateScopeRequest
    @ObservedObject var coordinator: UpdateScopeCoordinator

    private var leadCopy: String {
        req.current == nil
            ? "Set the scope for this session."
            : "Update the scope for this session."
    }

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: DialogTokens.sectionSpacing) {
                Text(leadCopy)
                    .foregroundStyle(DialogTokens.textSecondary)

                InfoSection(caption: "Current directory") {
                    HStack(spacing: 8) {
                        Image(systemName: "folder")
                            .font(.system(size: 12))
                            .foregroundStyle(DialogTokens.textSecondary)
                        Text(Branding.displayDir(req.working_dir))
                            .font(.system(.body, design: .monospaced))
                            .foregroundStyle(DialogTokens.textSecondary)
                    }
                }

                InfoSection(caption: "Scope") {
                    VStack(alignment: .leading, spacing: 10) {
                        HStack(spacing: 8) {
                            Image(systemName: "key")
                                .font(.system(size: 14))
                                .foregroundStyle(DialogTokens.textSecondary)
                            Text("What is the agent working on?")
                                .font(.system(size: 12))
                                .foregroundStyle(DialogTokens.textSecondary)
                        }
                        ScopeEditor(coordinator: coordinator)
                            .frame(height: 100)
                    }
                }
            }
            .padding(DialogTokens.outerPadding)

            DialogFooter(
                hints: [
                    DialogFooter.Hint(key: "RETURN", action: "save"),
                    DialogFooter.Hint(key: "OPT+RETURN", action: "newline"),
                    DialogFooter.Hint(key: "ESC", action: "cancel"),
                ]
            ) {
                HStack(spacing: 8) {
                    Button("Cancel", action: coordinator.cancel)
                        .buttonStyle(.bordered)
                    Button("Save", action: coordinator.commit)
                        .buttonStyle(.borderedProminent)
                }
            }
        }
        .frame(width: Branding.windowWidth, alignment: .top)
        .background(DialogTokens.appBackground)
        .task { coordinator.arm() }
    }
}

// MARK: - Entry point

enum UpdateScopeDialog {
    @MainActor
    static func run(_ req: UpdateScopeRequest) -> UpdateScopeResponse {
        let coordinator = UpdateScopeCoordinator(initialText: req.current ?? "")
        let view = UpdateScopeView(req: req, coordinator: coordinator)

        Dialog.runHosted(
            rootView: view,
            onArrow: coordinator.handleArrow,
            onReturn: coordinator.commit,
            onEsc: coordinator.cancel
        )

        return coordinator.response
    }
}
