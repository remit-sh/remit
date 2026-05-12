import AppKit
import Foundation

/// Verdict choice as a vertical menu — ↑/↓ to highlight, Return to confirm,
/// Esc to cancel. List rows: Deny (safe default), Allow, Update scope, Disable
/// session.
@MainActor
final class ChooseController: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    enum Choice: String, CaseIterable {
        case deny, allow, updateScope, disableSession

        var label: String {
            switch self {
            case .deny: return "Deny"
            case .allow: return "Allow"
            case .updateScope: return "Update scope"
            case .disableSession: return "Disable session"
            }
        }

        var wireValue: String {
            switch self {
            case .deny: return "deny"
            case .allow: return "allow"
            case .updateScope: return "update_scope"
            case .disableSession: return "disable_session"
            }
        }
    }

    private(set) var response: ChooseResponse = .cancelled
    weak var tableView: NSTableView?

    func numberOfRows(in tableView: NSTableView) -> Int { Choice.allCases.count }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let id = NSUserInterfaceItemIdentifier("ChoiceRow")
        let cell: NSTableCellView = (tableView.makeView(withIdentifier: id, owner: self) as? NSTableCellView)
            ?? {
                let v = NSTableCellView()
                let label = NSTextField(labelWithString: "")
                label.translatesAutoresizingMaskIntoConstraints = false
                label.font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
                v.addSubview(label)
                v.textField = label
                NSLayoutConstraint.activate([
                    label.leadingAnchor.constraint(equalTo: v.leadingAnchor, constant: 4),
                    label.trailingAnchor.constraint(equalTo: v.trailingAnchor, constant: -4),
                    label.centerYAnchor.constraint(equalTo: v.centerYAnchor),
                ])
                v.identifier = id
                return v
            }()
        cell.textField?.stringValue = Choice.allCases[row].label
        return cell
    }

    @objc func commitSelection() {
        guard let table = tableView else { return }
        let row = table.selectedRow
        if row >= 0 && row < Choice.allCases.count {
            response = .choice(Choice.allCases[row].wireValue)
            NSApp.stopModal()
        }
    }
}

/// Subclass that turns Return into "activate the row" — NSTableView ignores
/// Return by default but we want it to commit the highlighted choice.
@MainActor
final class ChoiceTableView: NSTableView {
    override func keyDown(with event: NSEvent) {
        if event.keyCode == 36 || event.keyCode == 76 {
            // Return / numpad Return — fire the table's action.
            if let target, let action {
                NSApp.sendAction(action, to: target, from: self)
                return
            }
        }
        super.keyDown(with: event)
    }
}

enum ChooseDialog {
    @MainActor
    static func run(_ req: ChooseRequest) -> ChooseResponse {
        let controller = ChooseController()
        let delegate = CloseAsCancelDelegate()

        let wordmark = Dialog.makeWordmark()
        let cwd = Dialog.makeLabel(
            Branding.displayDir(req.working_dir),
            font: NSFont.userFixedPitchFont(ofSize: NSFont.smallSystemFontSize),
            wraps: false
        )
        cwd.textColor = .secondaryLabelColor

        let reason = Dialog.makeFieldLabel(label: "Reason:", value: req.reason)
        let scope = Dialog.makeFieldLabel(label: "Scope:", value: req.scope)

        // Tool name becomes the field label — matches the **Reason:** / **Scope:**
        // pattern. Row omitted entirely if the hook didn't send the action
        // (older hooks; future placeholder).
        let toolRow: NSView? = {
            guard let name = req.tool_name, !name.isEmpty,
                let preview = req.args_preview, !preview.isEmpty
            else { return nil }
            return Dialog.makeFieldLabel(label: "\(name):", value: preview)
        }()

        let prompt = Dialog.makeLabel("Choose an action:")
        prompt.textColor = .secondaryLabelColor
        prompt.font = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)

        // Scrollless single-column NSTableView styled as a flat menu.
        let scroll = NSScrollView()
        scroll.borderType = .bezelBorder
        scroll.hasVerticalScroller = false
        scroll.hasHorizontalScroller = false
        scroll.drawsBackground = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        let table = ChoiceTableView()
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("choice"))
        column.title = ""
        column.resizingMask = .autoresizingMask
        table.addTableColumn(column)
        table.headerView = nil
        table.allowsMultipleSelection = false
        table.allowsEmptySelection = false
        table.rowHeight = 26
        table.intercellSpacing = NSSize(width: 0, height: 2)
        table.style = .plain
        table.usesAlternatingRowBackgroundColors = false
        table.gridStyleMask = []
        table.dataSource = controller
        table.delegate = controller
        controller.tableView = table

        table.target = controller
        table.action = #selector(ChooseController.commitSelection)
        table.doubleAction = #selector(ChooseController.commitSelection)

        scroll.documentView = table
        table.reloadData()
        // Row 0 = Deny. Pre-selected so an absent-minded Return keeps the
        // verdict closed — fail-safe end-to-end.
        table.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)

        let hint = Dialog.makeLabel(
            "↑↓ choose · Return confirm · Esc cancel",
            font: NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
        )
        hint.textColor = .tertiaryLabelColor
        hint.alignment = .center

        let cancelTarget = ChooseCancelTarget()
        let cancelButton = Dialog.makeButton(
            title: "Cancel",
            keyEquivalent: "\u{1B}",
            target: cancelTarget,
            action: #selector(ChooseCancelTarget.cancel)
        )

        let window = Dialog.makeWindow()

        // Table height: rowHeight × rows + intercell spacing per row +
        // top/bottom inner padding so the rounded selection highlight isn't
        // clipped against the bezel border.
        let innerPadding: CGFloat = 8
        let tableHeight: CGFloat = CGFloat(ChooseController.Choice.allCases.count) * (table.rowHeight + 2) + innerPadding
        scroll.heightAnchor.constraint(equalToConstant: tableHeight).isActive = true
        scroll.widthAnchor.constraint(
            equalToConstant: Branding.windowWidth - 2 * Branding.edgeInset
        ).isActive = true
        hint.widthAnchor.constraint(
            equalToConstant: Branding.windowWidth - 2 * Branding.edgeInset
        ).isActive = true

        var contentViews: [NSView] = [wordmark, cwd]
        if let toolRow {
            contentViews.append(toolRow)
        }
        contentViews.append(contentsOf: [reason, scope, prompt, scroll, hint])

        Dialog.assemble(
            window: window,
            delegate: delegate,
            contentViews: contentViews,
            customSpacings: [
                (after: wordmark, spacing: Branding.headerSpacing),
                (after: scope, spacing: 16),
                (after: prompt, spacing: 4),
                (after: scroll, spacing: 6),
            ],
            buttons: [cancelButton],
            defaultFocus: table
        )

        // AppKit holds button.target / window.delegate / table.dataSource
        // weakly. `withExtendedLifetime` guarantees ARC keeps these alive
        // for the full modal session, defending against optimisation that
        // could otherwise drop them once the last source-line reference
        // passes.
        _ = withExtendedLifetime((controller, delegate, cancelTarget)) {
            NSApp.runModal(for: window)
        }
        return controller.response
    }
}

/// Cancel target for the bottom-bar Cancel button — kept separate so
/// `ChooseController` stays focused on table-row selection logic.
@MainActor
final class ChooseCancelTarget: NSObject {
    @objc func cancel() { NSApp.stopModal(withCode: .cancel) }
}
