#if os(iOS)
import ActivityKit
import CryptoKit
import Foundation

/// The Live Activity of what the agents do. The auth server starts and updates it by push with
/// `"attributes-type": "AgentsAttributes"`; each part is a `ServerWork` sealed to this phone.
public struct AgentsAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable, Sendable {
        public let parts: [String]
        /// How many work and wait across every server, which count right even when parts don't open.
        public let working: Int?
        public let waiting: Int?

        public init(parts: [String], working: Int? = nil, waiting: Int? = nil) {
            self.parts = parts
            self.working = working
            self.waiting = waiting
        }
    }

    public init() {}
}

extension AgentsAttributes.ContentState {
    /// The parts this phone opens, merged across its servers.
    public func work(with key: Curve25519.KeyAgreement.PrivateKey? = PushSecret.load()) -> AgentsWork {
        let servers = parts.compactMap { part -> ServerWork? in
            guard let key, case .work(let work) = PushOpener.open(sealed: part, with: key) else { return nil }
            return work
        }
        return AgentsWork(servers, working: working, waiting: waiting)
    }
}
#endif
