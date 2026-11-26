import CryptoKit
import Foundation

/// Opens what a server sealed to this phone (crates/protocol's `alerts::seal`): HPKE base mode with
/// X25519, HKDF-SHA256 and ChaCha20-Poly1305, in base64 as the encapsulated key and the ciphertext.
public enum PushOpener {
    private static let info = Data("motile push 1".utf8)
    private static let encapsulatedKeyLength = 32

    public static func open(sealed: String, with key: Curve25519.KeyAgreement.PrivateKey? = PushSecret.load()) -> Pushed? {
        guard let key, let json = openBytes(sealed: sealed, with: key) else { return nil }
        return Pushed(json: json)
    }

    public static func openBytes(sealed: String, with key: Curve25519.KeyAgreement.PrivateKey) -> Data? {
        guard let bytes = Data(base64Encoded: sealed.trimmingCharacters(in: .whitespacesAndNewlines)),
              bytes.count > encapsulatedKeyLength else { return nil }
        let encapsulatedKey = bytes.prefix(encapsulatedKeyLength)
        let ciphertext = bytes.dropFirst(encapsulatedKeyLength)
        guard var recipient = try? HPKE.Recipient(
            privateKey: key, ciphersuite: .Curve25519_SHA256_ChachaPoly, info: info, encapsulatedKey: encapsulatedKey
        ) else { return nil }
        return try? recipient.open(ciphertext)
    }
}
