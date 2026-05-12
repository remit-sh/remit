import AppKit
import Foundation

/// "Disable Remit for this session?" yes/no confirm. Cancel is the default,
/// matching destructive-action HIG.
@MainActor
final class ConfirmDisableController: NSObject {
    private(set) var confirmed: Bool = false

    @objc func confirm() { confirmed = true; NSApp.stopModal() }
    @objc func cancel() { confirmed = false; NSApp.stopModal() }
}

enum ConfirmDisableDialog {
    @MainActor
    static func run() -> ConfirmDisableResponse {
        let controller = ConfirmDisableController()
        let delegate = CloseAsCancelDelegate()

        let wordmark = Dialog.makeWordmark()
        let heading = Dialog.makeLabel("Disable Remit for the rest of this session?")
        heading.font = NSFont.boldSystemFont(ofSize: NSFont.systemFontSize)

        let body = Dialog.makeLabel(
            "Classification will be skipped until the Claude Code session ends."
        )
        body.textColor = .secondaryLabelColor

        let disableButton = Dialog.makeButton(
            title: "Disable",
            keyEquivalent: "",
            target: controller,
            action: #selector(ConfirmDisableController.confirm)
        )
        let cancelButton = Dialog.makeButton(
            title: "Cancel",
            keyEquivalent: "\r",
            target: controller,
            action: #selector(ConfirmDisableController.cancel)
        )

        let window = Dialog.makeWindow()
        Dialog.assemble(
            window: window,
            delegate: delegate,
            contentViews: [wordmark, heading, body],
            customSpacings: [(after: wordmark, spacing: Branding.headerSpacing)],
            buttons: [disableButton, cancelButton],
            defaultFocus: cancelButton
        )

        _ = withExtendedLifetime((controller, delegate)) {
            NSApp.runModal(for: window)
        }
        return ConfirmDisableResponse(confirmed: controller.confirmed)
    }
}
