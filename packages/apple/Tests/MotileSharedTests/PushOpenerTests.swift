import CryptoKit
import Foundation
import Testing

@testable import MotileShared

/// `Fixtures/sealed.json` was sealed by crates/protocol's `alerts::seal_pushed`, to the device key
/// whose X25519 secret is `secret`.
private struct Fixture: Decodable {
    let secret: String
    let alert: String
    let work: String
    let otherDeviceAlert: String

    static let shared: Fixture = {
        let url = Bundle.module.url(forResource: "sealed", withExtension: "json", subdirectory: "Fixtures")!
        return try! Pushed.decoder.decode(Fixture.self, from: Data(contentsOf: url))
    }()

    var key: Curve25519.KeyAgreement.PrivateKey {
        try! Curve25519.KeyAgreement.PrivateKey(rawRepresentation: Data(base64Encoded: secret)!)
    }
}

@Test func opensAnAlertSealedByTheServer() {
    let fixture = Fixture.shared
    guard case .alert(let alert) = PushOpener.open(sealed: fixture.alert, with: fixture.key) else {
        Issue.record("the alert didn't open")
        return
    }
    #expect(alert.threadId == "thread-1")
    #expect(alert.serverId == "server-1")
    #expect(alert.kind == .approval)
    #expect(alert.threadTitle == "Fix the login")
    #expect(alert.project == "motile")
    #expect(alert.detail == "Run greet.py")
    #expect(alert.body == "Needs your approval · Run greet.py")
}

@Test func opensWorkSealedByTheServer() {
    let fixture = Fixture.shared
    guard case .work(let work) = PushOpener.open(sealed: fixture.work, with: fixture.key) else {
        Issue.record("the work didn't open")
        return
    }
    #expect(work.serverName == "studio")
    #expect(work.items.map(\.state) == [.waiting, .working, .finished])
    #expect((work.working, work.waiting) == (1, 1))
}

@Test func doesNotOpenWhatWasSealedToAnotherDeviceOrTamperedWith() {
    let fixture = Fixture.shared
    #expect(PushOpener.open(sealed: fixture.otherDeviceAlert, with: fixture.key) == nil)
    var bytes = Data(base64Encoded: fixture.alert)!
    bytes[bytes.count - 1] ^= 1
    #expect(PushOpener.open(sealed: bytes.base64EncodedString(), with: fixture.key) == nil)
    #expect(PushOpener.open(sealed: "not base64", with: fixture.key) == nil)
    #expect(PushOpener.open(sealed: Data(count: 8).base64EncodedString(), with: fixture.key) == nil)
}
