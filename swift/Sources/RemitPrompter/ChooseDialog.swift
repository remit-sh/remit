import AppKit
import SwiftUI

// Red-verdict prompt: warning banner over ACTION / ACTION ARGS / AGENT SCOPE
// context, with a keyboard-driven picker of Deny / Allow / Update scope /
// Disable session. First validation of the hosted SwiftUI pattern.

// MARK: - Options

struct ChooseOption {
    let icon: String
    let label: String
    let wireValue: String
    let isDestructive: Bool
}

// MARK: - Coordinator

@MainActor
final class ChooseCoordinator: ObservableObject {
    static let options: [ChooseOption] = [
        ChooseOption(icon: "nosign", label: "Deny", wireValue: "deny", isDestructive: false),
        ChooseOption(
            icon: "checkmark.circle", label: "Allow", wireValue: "allow", isDestructive: false
        ),
        ChooseOption(
            icon: "key", label: "Update scope", wireValue: "update_scope", isDestructive: false
        ),
        ChooseOption(
            icon: "rectangle.portrait.and.arrow.right",
            label: "Disable session",
            wireValue: "disable_session",
            isDestructive: true
        ),
    ]

    @Published var selectedIndex: Int = 0
    @Published var inputArmed: Bool = false

    private(set) var response: ChooseResponse = .cancelled

    /// Start the 1s input-arming timer. Until it fires, every key handler
    /// and Esc / title-bar X early-returns — protects against a terminal
    /// Enter dismissing the dialog before the user reads it.
    func arm() {
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 1_000_000_000)
            self.inputArmed = true
        }
    }

    func handleArrow(_ direction: ArrowDirection) {
        guard inputArmed else { return }
        switch direction {
        case .up:
            if selectedIndex > 0 { selectedIndex -= 1 }
        case .down:
            if selectedIndex < Self.options.count - 1 { selectedIndex += 1 }
        case .left, .right:
            break
        }
    }

    func commit() {
        guard inputArmed else { return }
        let option = Self.options[selectedIndex]
        response = .choice(option.wireValue)
        NSApp.stopModal()
    }

    func cancel() {
        guard inputArmed else { return }
        response = .cancelled
        NSApp.stopModal()
    }
}

// MARK: - View

struct ChooseView: View {
    let req: ChooseRequest
    @ObservedObject var coordinator: ChooseCoordinator

    var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: DialogTokens.sectionSpacing) {
                WarningBanner(reason: req.reason)

                InfoSection(caption: "Agent scope") {
                    AgentScopeRow(scope: req.scope)
                }

                InfoSection(caption: "Action") {
                    ActionRow(
                        workingDir: req.working_dir,
                        toolName: req.tool_name ?? "Action"
                    )
                }

                if let args = req.args_pretty, !args.isEmpty {
                    InfoSection(caption: "Action args") {
                        ArgsScrollBox(args: args)
                    }
                }

                PickerSection(coordinator: coordinator)
            }
            .padding(DialogTokens.outerPadding)

            DialogFooter(hints: [
                DialogFooter.Hint(key: "↑↓", action: "choose"),
                DialogFooter.Hint(key: "RETURN", action: "confirm"),
                DialogFooter.Hint(key: "ESC", action: "cancel"),
            ])
        }
        .frame(width: Branding.windowWidth, alignment: .top)
        .background(DialogTokens.appBackground)
        .task { coordinator.arm() }
    }
}

// MARK: - Subviews

private struct WarningBanner: View {
    let reason: String

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "hand.raised.fill")
                .font(.system(size: 16))
                .foregroundStyle(DialogTokens.destructive)
            VStack(alignment: .leading, spacing: 4) {
                Text("Remit has held this action")
                    .font(.headline)
                    .foregroundStyle(DialogTokens.textPrimary)
                Text("REASON: \(reason)")
                    .font(.system(.subheadline, design: .monospaced))
                    .foregroundStyle(DialogTokens.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(DialogTokens.surfacePadding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(DialogTokens.warningSurface)
        .overlay(
            RoundedRectangle(cornerRadius: DialogTokens.cornerRadius)
                .stroke(DialogTokens.warningStroke, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: DialogTokens.cornerRadius))
    }
}

private struct ActionRow: View {
    let workingDir: String
    let toolName: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "folder")
                    .font(.system(size: 12))
                    .foregroundStyle(DialogTokens.textSecondary)
                Text("Working dir:")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(DialogTokens.textSecondary)
                Text(Branding.displayDir(workingDir))
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(DialogTokens.textSecondary)
            }
            Text(toolName)
                .font(.headline)
                .foregroundStyle(DialogTokens.textPrimary)
        }
    }
}

private struct ArgsScrollBox: View {
    let args: String

    var body: some View {
        ScrollView(.vertical) {
            Text(args)
                .font(.system(.body, design: .monospaced))
                .foregroundStyle(DialogTokens.textPrimary)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(minHeight: 40, maxHeight: 140)
    }
}

private struct AgentScopeRow: View {
    let scope: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "lock.shield")
                .font(.system(size: 14))
                .foregroundStyle(DialogTokens.textSecondary)
            Text(scope)
                .font(.headline)
                .foregroundStyle(DialogTokens.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

private struct PickerSection: View {
    @ObservedObject var coordinator: ChooseCoordinator

    var body: some View {
        VStack(alignment: .leading, spacing: DialogTokens.captionSpacing) {
            Text("Choose an action:")
                .font(DialogTokens.captionFont)
                .tracking(0.5)
                .foregroundStyle(DialogTokens.textSecondary)
                .textCase(.uppercase)

            VStack(spacing: 2) {
                ForEach(Array(ChooseCoordinator.options.enumerated()), id: \.offset) { idx, option in
                    Button {
                        coordinator.selectedIndex = idx
                        coordinator.commit()
                    } label: {
                        PickerRow(
                            icon: option.icon,
                            label: option.label,
                            isDestructive: option.isDestructive,
                            isSelected: idx == coordinator.selectedIndex
                        )
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(4)
            .background(DialogTokens.surface)
            .clipShape(RoundedRectangle(cornerRadius: DialogTokens.cornerRadius))
        }
    }
}

// MARK: - Entry point

enum ChooseDialog {
    @MainActor
    static func run(_ req: ChooseRequest) -> ChooseResponse {
        let coordinator = ChooseCoordinator()
        let view = ChooseView(req: req, coordinator: coordinator)

        Dialog.runHosted(
            rootView: view,
            onArrow: coordinator.handleArrow,
            onReturn: coordinator.commit,
            onEsc: coordinator.cancel
        )

        return coordinator.response
    }
}
