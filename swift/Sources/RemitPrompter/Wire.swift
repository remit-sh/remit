import Foundation

/// JSON wire protocol between the Rust hook (`remit-claude-code-hook`) and this
/// Swift binary. One `Request` per stdin, one `Response` per stdout, exit code
/// 0 on success / 2 on protocol error / 3 on headless (no WindowServer).
///
/// Forward compatibility: unknown JSON keys are ignored on both sides. Protocol
/// version bumps are deliberate breaks — both sides log on mismatch.

let protocolVersion: Int = 1

enum WireError: Error, CustomStringConvertible {
    case malformedJSON(String)
    case protocolMismatch(saw: Int)
    case unknownKind(String)

    var description: String {
        switch self {
        case .malformedJSON(let msg): return "malformed request JSON: \(msg)"
        case .protocolMismatch(let saw): return "protocol mismatch: adapter sent \(saw), expected \(protocolVersion)"
        case .unknownKind(let k): return "unknown request kind: \(k)"
        }
    }
}

/// All requests are tagged unions on `kind` with shared `protocol` discriminator.
struct RequestEnvelope: Decodable {
    let `protocol`: Int
    let kind: String
}

struct ChooseRequest: Decodable {
    let scope: String
    let reason: String
    let working_dir: String
    /// Tool name of the [`Action`] being authorised. Optional so requests from
    /// older hooks still decode; the dialog omits the row when absent.
    let tool_name: String?
    /// Truncated args value the adapter selected via its per-tool allowlist.
    /// Optional for the same backward-decode reason as `tool_name`.
    let args_preview: String?
}

struct UpdateScopeRequest: Decodable {
    /// Absent ⇒ first-time scope setup; empty default in the text field.
    let current: String?
    let working_dir: String
}

struct ConfirmDisableRequest: Decodable {}

// MARK: - Responses
//
// All responses are tagged with `kind` so the adapter can refuse a reply that
// doesn't match the request (defends against wire-protocol drift and
// version skew). The encoder is configured to omit `nil` Optionals so the
// presence of a key carries information.

private let responseEncoder: JSONEncoder = {
    let e = JSONEncoder()
    return e
}()

struct ChooseResponse: Encodable {
    let kind: String
    let choice: String?
    let cancelled: Bool?
    static func choice(_ c: String) -> Self { .init(kind: "choose", choice: c, cancelled: nil) }
    static var cancelled: Self { .init(kind: "choose", choice: nil, cancelled: true) }

    enum CodingKeys: String, CodingKey { case kind, choice, cancelled }
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(kind, forKey: .kind)
        if let choice { try c.encode(choice, forKey: .choice) }
        if let cancelled { try c.encode(cancelled, forKey: .cancelled) }
    }
}

struct UpdateScopeResponse: Encodable {
    let kind: String
    let scope: String?
    let cancelled: Bool?
    static func scope(_ s: String) -> Self {
        .init(kind: "update_scope", scope: s, cancelled: nil)
    }
    static var cancelled: Self { .init(kind: "update_scope", scope: nil, cancelled: true) }

    enum CodingKeys: String, CodingKey { case kind, scope, cancelled }
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(kind, forKey: .kind)
        if let scope { try c.encode(scope, forKey: .scope) }
        if let cancelled { try c.encode(cancelled, forKey: .cancelled) }
    }
}

struct ConfirmDisableResponse: Encodable {
    let kind: String = "confirm_disable"
    let confirmed: Bool
    init(confirmed: Bool) { self.confirmed = confirmed }
}

func emit<T: Encodable>(_ value: T) throws {
    let data = try responseEncoder.encode(value)
    FileHandle.standardOutput.write(data)
}
