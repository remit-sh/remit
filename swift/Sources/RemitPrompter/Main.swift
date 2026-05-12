import AppKit
import Foundation

/// Entry point: read one JSON request from stdin, run the matching dialog,
/// write one JSON response to stdout, exit.
///
/// Exit codes:
///   0 — normal completion (cancellations included; they're explicit responses)
///   2 — protocol error (malformed JSON, unknown kind, protocol-version mismatch)
///   3 — headless / no WindowServer available
///   non-zero from a crash — adapter maps to fail-closed Red

@main
struct RemitPrompter {
    @MainActor
    static func main() {
        exit(run())
    }
}

@MainActor
private func run() -> Int32 {
    let stdinData = FileHandle.standardInput.readDataToEndOfFile()
    if stdinData.count > 64 * 1024 {
        FileHandle.standardError.write(Data("request exceeds 64 KiB cap\n".utf8))
        return 2
    }

    let decoder = JSONDecoder()
    let envelope: RequestEnvelope
    do {
        envelope = try decoder.decode(RequestEnvelope.self, from: stdinData)
    } catch {
        FileHandle.standardError.write(Data("malformed request: \(error)\n".utf8))
        return 2
    }

    guard envelope.protocol == protocolVersion else {
        FileHandle.standardError.write(
            Data("protocol mismatch: adapter sent \(envelope.protocol), expected \(protocolVersion)\n".utf8)
        )
        return 2
    }

    if !Dialog.ensureAppKitReady() {
        FileHandle.standardError.write(Data("no WindowServer / GUI session\n".utf8))
        return 3
    }

    do {
        switch envelope.kind {
        case "choose":
            let req = try decoder.decode(ChooseRequest.self, from: stdinData)
            try emit(ChooseDialog.run(req))
        case "update_scope":
            let req = try decoder.decode(UpdateScopeRequest.self, from: stdinData)
            try emit(UpdateScopeDialog.run(req))
        case "confirm_disable":
            try emit(ConfirmDisableDialog.run())
        default:
            FileHandle.standardError.write(Data("unknown request kind: \(envelope.kind)\n".utf8))
            return 2
        }
    } catch {
        FileHandle.standardError.write(Data("dialog failed: \(error)\n".utf8))
        return 1
    }
    return 0
}
