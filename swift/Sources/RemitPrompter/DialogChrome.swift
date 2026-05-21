import AppKit
import SwiftUI

// SwiftUI chrome shared by all three dialogs, plus the AppKit bridge that
// drives them (`KeyHandlingHostingView`). Token values live in
// `DialogTokens`; view primitives compose them.

// MARK: - Tokens

enum DialogTokens {
    // Surfaces
    static let appBackground = Color(red: 0.05, green: 0.07, blue: 0.09)
    static let surface = Color.white.opacity(0.05)
    static let warningSurface = Color.red.opacity(0.15)
    static let warningStroke = Color.red
    static let selection = Color(red: 0.33, green: 0.41, blue: 1.0)

    // Text
    static let textPrimary = Color.white
    static let textSecondary = Color.secondary
    static let destructive = Color.red

    // Geometry
    static let cornerRadius: CGFloat = 8
    static let outerPadding: CGFloat = 16
    static let sectionSpacing: CGFloat = 14
    static let captionSpacing: CGFloat = 6
    static let surfacePadding: CGFloat = 14

    // Keycap
    static let keycapFill = Color.white.opacity(0.08)
    static let keycapStroke = Color.white.opacity(0.15)
    static let keycapCorner: CGFloat = 4

    // Fonts
    static let captionFont = Font.caption2.bold()
    static let keycapFont = Font.system(.caption, design: .monospaced)
}

// MARK: - Title bar

struct TitleBarWordmark: View {
    var body: some View {
        if let image = Branding.wordmark() {
            Image(nsImage: image)
                .resizable()
                .scaledToFit()
                .frame(height: 14)
                .opacity(0.9)
        }
    }
}

struct TitleBarClose: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "xmark")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(DialogTokens.textSecondary)
                .frame(width: 22, height: 22)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: - InfoSection

struct InfoSection<Content: View>: View {
    let caption: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: DialogTokens.captionSpacing) {
            Text(caption)
                .font(DialogTokens.captionFont)
                .tracking(0.5)
                .foregroundStyle(DialogTokens.textSecondary)
                .textCase(.uppercase)

            content()
                .padding(DialogTokens.surfacePadding)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(DialogTokens.surface)
                .clipShape(RoundedRectangle(cornerRadius: DialogTokens.cornerRadius))
        }
    }
}

// MARK: - KeycapHint

struct KeycapHint: View {
    let key: String
    let action: String

    var body: some View {
        HStack(spacing: 6) {
            Text(key)
                .font(DialogTokens.keycapFont)
                .foregroundStyle(DialogTokens.textPrimary)
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(
                    RoundedRectangle(cornerRadius: DialogTokens.keycapCorner)
                        .fill(DialogTokens.keycapFill)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: DialogTokens.keycapCorner)
                        .stroke(DialogTokens.keycapStroke, lineWidth: 1)
                )
            Text(action)
                .font(.caption)
                .foregroundStyle(DialogTokens.textSecondary)
        }
    }
}

// MARK: - DialogFooter

struct DialogFooter: View {
    struct Hint {
        let key: String
        let action: String
    }

    let hints: [Hint]
    let trailing: AnyView?

    init(hints: [Hint]) {
        self.hints = hints
        self.trailing = nil
    }

    init<T: View>(hints: [Hint], @ViewBuilder trailing: () -> T) {
        self.hints = hints
        self.trailing = AnyView(trailing())
    }

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            HStack(spacing: 0) {
                HStack(spacing: 12) {
                    ForEach(Array(hints.enumerated()), id: \.offset) { idx, hint in
                        if idx > 0 {
                            Text("·").foregroundStyle(DialogTokens.textSecondary)
                        }
                        KeycapHint(key: hint.key, action: hint.action)
                    }
                }
                Spacer(minLength: 12)
                if let trailing {
                    trailing
                }
            }
            .padding(.horizontal, DialogTokens.outerPadding)
            .padding(.vertical, 12)
        }
    }
}

// MARK: - PickerRow

struct PickerRow: View {
    let icon: String
    let label: String
    let isDestructive: Bool
    let isSelected: Bool

    init(icon: String, label: String, isDestructive: Bool = false, isSelected: Bool = false) {
        self.icon = icon
        self.label = label
        self.isDestructive = isDestructive
        self.isSelected = isSelected
    }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.system(size: 16, weight: .regular))
                .frame(width: 20)
            Text(label)
                .font(.system(size: 14, weight: .medium))
            Spacer()
            if isSelected {
                Text("ENTER")
                    .font(DialogTokens.keycapFont)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(
                        RoundedRectangle(cornerRadius: DialogTokens.keycapCorner)
                            .fill(Color.white.opacity(0.15))
                    )
            }
        }
        .foregroundStyle(textColor)
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(isSelected ? DialogTokens.selection : Color.clear)
        .clipShape(RoundedRectangle(cornerRadius: 6))
        .contentShape(Rectangle())
    }

    private var textColor: Color {
        if isSelected { return .white }
        if isDestructive { return DialogTokens.destructive }
        return DialogTokens.textPrimary
    }
}

// MARK: - AppKit bridge: KeyHandlingHostingView

enum ArrowDirection {
    case up, down, left, right
}

// `NSHostingView` subclass that maps arrow / Return / Esc keys to caller-
// supplied closures. Anything else falls through to `super`, so a focused
// `TextEditor` keeps its normal text-editing behaviour.
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
