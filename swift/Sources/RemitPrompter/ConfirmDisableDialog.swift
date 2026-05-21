import AppKit
import SwiftUI

// "Disable Remit for this session?" confirm. Cancel is the default; Disable
// (red destructive) sits beside it. Reaching Disable takes `→` then Return.
// No input arming: this dialog only appears after explicit picker navigation
// in the choose dialog, so there's no terminal-race risk.

// MARK: - Coordinator

enum ConfirmDisableButton {
    case cancel
    case disable
}

@MainActor
final class ConfirmDisableCoordinator: ObservableObject {
    @Published var selectedButton: ConfirmDisableButton = .cancel

    private(set) var response: ConfirmDisableResponse = ConfirmDisableResponse(confirmed: false)

    func handleArrow(_ direction: ArrowDirection) {
        switch direction {
        case .left: selectedButton = .cancel
        case .right: selectedButton = .disable
        case .up, .down: break
        }
    }

    func commit() {
        switch selectedButton {
        case .cancel: cancel()
        case .disable: disable()
        }
    }

    func cancel() {
        response = ConfirmDisableResponse(confirmed: false)
        NSApp.stopModal()
    }

    func disable() {
        response = ConfirmDisableResponse(confirmed: true)
        NSApp.stopModal()
    }
}

// MARK: - View

struct ConfirmDisableView: View {
    @ObservedObject var coordinator: ConfirmDisableCoordinator

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                Text("Disable Remit for the rest of this session?")
                    .font(.headline)
                    .foregroundStyle(DialogTokens.textPrimary)
                Text("Classification will be skipped until the Claude Code session ends.")
                    .foregroundStyle(DialogTokens.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(DialogTokens.outerPadding)

            DialogFooter(
                hints: [
                    DialogFooter.Hint(key: "←→", action: "choose"),
                    DialogFooter.Hint(key: "RETURN", action: "confirm"),
                    DialogFooter.Hint(key: "ESC", action: "cancel"),
                ]
            ) {
                HStack(spacing: 8) {
                    Button("Cancel", action: coordinator.cancel)
                        .buttonStyle(
                            SelectableButtonStyle(
                                isSelected: coordinator.selectedButton == .cancel,
                                isDestructive: false
                            )
                        )
                    Button("Disable", action: coordinator.disable)
                        .buttonStyle(
                            SelectableButtonStyle(
                                isSelected: coordinator.selectedButton == .disable,
                                isDestructive: true
                            )
                        )
                }
            }
        }
        .frame(width: Branding.windowWidth, alignment: .top)
        .background(DialogTokens.appBackground)
    }
}

// MARK: - Button style

private struct SelectableButtonStyle: ButtonStyle {
    let isSelected: Bool
    let isDestructive: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: isSelected ? .semibold : .regular))
            .foregroundStyle(textColor)
            .padding(.horizontal, 14)
            .padding(.vertical, 6)
            .background(background)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .stroke(borderColor, lineWidth: 1)
            )
            .scaleEffect(configuration.isPressed ? 0.97 : 1.0)
    }

    private var textColor: Color {
        if isDestructive {
            return isSelected ? .white : DialogTokens.destructive
        }
        return DialogTokens.textPrimary
    }

    private var background: Color {
        if isSelected {
            return isDestructive ? DialogTokens.destructive : DialogTokens.selection
        }
        return Color.white.opacity(0.06)
    }

    private var borderColor: Color {
        if isSelected { return .clear }
        if isDestructive { return DialogTokens.destructive.opacity(0.5) }
        return Color.white.opacity(0.15)
    }
}

// MARK: - Entry point

enum ConfirmDisableDialog {
    @MainActor
    static func run() -> ConfirmDisableResponse {
        let coordinator = ConfirmDisableCoordinator()
        let view = ConfirmDisableView(coordinator: coordinator)

        Dialog.runHosted(
            rootView: view,
            onArrow: coordinator.handleArrow,
            onReturn: coordinator.commit,
            onEsc: coordinator.cancel
        )

        return coordinator.response
    }
}
