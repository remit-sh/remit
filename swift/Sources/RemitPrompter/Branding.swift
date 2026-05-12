import AppKit
import Foundation

/// Shared visual constants and asset loading. Layout values are chosen so all
/// three dialogs share the same window width and button-row geometry, giving
/// the three prompts visual continuity.
enum Branding {
    static let windowWidth: CGFloat = 560
    static let edgeInset: CGFloat = 24
    static let stackSpacing: CGFloat = 12
    static let headerSpacing: CGFloat = 6   // wordmark → separator
    static let buttonSpacing: CGFloat = 10
    static let bottomBarHeight: CGFloat = 56  // native sheet-style button bar
    static let wordmarkSize = NSSize(width: 140, height: 32)

    /// Loads the embedded wordmark. Returns `nil` only if the asset is missing
    /// from the bundle (build-system bug). Dialogs handle nil by omitting the
    /// image rather than crashing.
    static func wordmark() -> NSImage? {
        guard let url = Bundle.module.url(forResource: "Wordmark", withExtension: "png") else {
            return nil
        }
        return NSImage(contentsOf: url)
    }

    /// Tilde-collapse a working-dir path for display. Matches `display_dir` in
    /// the Rust adapter so the two prompters render paths identically.
    static func displayDir(_ raw: String) -> String {
        let home = NSHomeDirectory()
        guard !home.isEmpty, raw.hasPrefix(home) else { return raw }
        let rest = String(raw.dropFirst(home.count))
        if rest.isEmpty { return "~" }
        if rest.hasPrefix("/") { return "~" + rest }
        return "~/" + rest
    }
}
