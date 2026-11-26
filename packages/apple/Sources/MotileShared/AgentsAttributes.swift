#if os(iOS)
import ActivityKit
import CryptoKit
import Foundation

/// The Live Activity of what the agents do. The auth server starts and updates it by push with
/// `"attributes-type": "AgentsAttributes"`; each part is a `ServerWork` sealed to this phone.
public struct AgentsAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable, Sendable {
        public let parts: [String]

        public init(parts: [String]) {
            self.parts = parts
        }
    }

    public init() {}
}

extension AgentsAttributes.ContentState {
    /// The parts this phone opens, merged across its servers.
    public func work(with key: Curve25519.KeyAgreement.PrivateKey? = PushSecret.load()) -> AgentsWork {
        guard let key else { return AgentsWork([]) }
        let servers = parts.compactMap { part -> ServerWork? in
            guard case .work(let work) = PushOpener.open(sealed: part, with: key) else { return nil }
            return work
        }
        return AgentsWork(servers)
    }
}
#endif
