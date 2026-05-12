import Foundation
import Testing
@testable import RemitPrompter

private let decoder = JSONDecoder()
private let encoder = JSONEncoder()

// MARK: - Envelope

@Test func envelopeDecodesProtocolAndKind() throws {
    let json = #"{"protocol": 1, "kind": "choose", "extra": "ignored"}"#
    let env = try decoder.decode(RequestEnvelope.self, from: Data(json.utf8))
    #expect(env.protocol == 1)
    #expect(env.kind == "choose")
}

// MARK: - ChooseRequest

@Test func chooseRequestDecodes() throws {
    let json = #"""
    {
        "protocol": 1,
        "kind": "choose",
        "scope": "Agenthoff dev work",
        "reason": "touches ~/.ssh",
        "working_dir": "/Users/x/dev/y"
    }
    """#
    let req = try decoder.decode(ChooseRequest.self, from: Data(json.utf8))
    #expect(req.scope == "Agenthoff dev work")
    #expect(req.reason == "touches ~/.ssh")
    #expect(req.working_dir == "/Users/x/dev/y")
}

@Test func chooseRequestIgnoresUnknownKeys() throws {
    let json = #"""
    {
        "protocol": 1,
        "kind": "choose",
        "scope": "s",
        "reason": "r",
        "working_dir": "/w",
        "future_field": [1, 2, 3]
    }
    """#
    _ = try decoder.decode(ChooseRequest.self, from: Data(json.utf8))
}

// MARK: - UpdateScopeRequest

@Test func updateScopeRequestWithCurrent() throws {
    let json = #"{"protocol":1,"kind":"update_scope","current":"old","working_dir":"/w"}"#
    let req = try decoder.decode(UpdateScopeRequest.self, from: Data(json.utf8))
    #expect(req.current == "old")
    #expect(req.working_dir == "/w")
}

@Test func updateScopeRequestFirstTimeOmitsCurrent() throws {
    let json = #"{"protocol":1,"kind":"update_scope","working_dir":"/w"}"#
    let req = try decoder.decode(UpdateScopeRequest.self, from: Data(json.utf8))
    #expect(req.current == nil)
}

// MARK: - Responses

@Test func chooseResponseChoiceEncodes() throws {
    let data = try encoder.encode(ChooseResponse.choice("allow"))
    let s = String(data: data, encoding: .utf8)!
    #expect(s.contains("\"choice\":\"allow\""))
    #expect(!s.contains("\"cancelled\""))
}

@Test func chooseResponseCancelledEncodes() throws {
    let data = try encoder.encode(ChooseResponse.cancelled)
    let s = String(data: data, encoding: .utf8)!
    #expect(s.contains("\"cancelled\":true"))
    #expect(!s.contains("\"choice\""))
}

@Test func updateScopeResponseScopeEncodes() throws {
    let data = try encoder.encode(UpdateScopeResponse.scope("new scope"))
    let s = String(data: data, encoding: .utf8)!
    #expect(s.contains("\"scope\":\"new scope\""))
}

@Test func updateScopeResponseCancelledEncodes() throws {
    let data = try encoder.encode(UpdateScopeResponse.cancelled)
    let s = String(data: data, encoding: .utf8)!
    #expect(s.contains("\"cancelled\":true"))
}

@Test func confirmDisableResponseEncodes() throws {
    let yes = try encoder.encode(ConfirmDisableResponse(confirmed: true))
    #expect(String(data: yes, encoding: .utf8)!.contains("\"confirmed\":true"))

    let no = try encoder.encode(ConfirmDisableResponse(confirmed: false))
    #expect(String(data: no, encoding: .utf8)!.contains("\"confirmed\":false"))
}

// MARK: - Branding

@Test func displayDirTildeCollapsesHome() {
    let home = NSHomeDirectory()
    #expect(Branding.displayDir("\(home)/dev/x") == "~/dev/x")
}

@Test func displayDirExactHomeBecomesTilde() {
    #expect(Branding.displayDir(NSHomeDirectory()) == "~")
}

@Test func displayDirPathOutsideHomeUnchanged() {
    #expect(Branding.displayDir("/usr/local/bin") == "/usr/local/bin")
}
