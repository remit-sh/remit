import AppKit
import Foundation

/// Shared visual constants and asset loading. Fixed window width gives the
/// three prompts visual continuity.
enum Branding {
    static let windowWidth: CGFloat = 560

    /// Loads the embedded wordmark. Returns `nil` only if the asset is missing
    /// from the bundle (build-system bug). Callers handle nil by omitting the
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
