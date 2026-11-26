import CryptoKit
import Foundation
import Security

/// This device's key as an X25519 secret, which the app gets from the core and keeps in the
/// shared keychain group, so that the extensions open pushes without the core.
public enum PushSecret {
    private static let service = "app.motile.push"
    private static let account = "device"

    @discardableResult
    public static func store(base64: String) -> Bool {
        guard let secret = Data(base64Encoded: base64), secret.count == 32 else { return false }
        guard load()?.rawRepresentation != secret else { return true }
        SecItemDelete(query() as CFDictionary)
        var item = query()
        item[kSecValueData as String] = secret
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    public static func load() -> Curve25519.KeyAgreement.PrivateKey? {
        var search = query()
        search[kSecReturnData as String] = true
        search[kSecMatchLimit as String] = kSecMatchLimitOne
        var found: CFTypeRef?
        guard SecItemCopyMatching(search as CFDictionary, &found) == errSecSuccess, let secret = found as? Data else { return nil }
        return try? Curve25519.KeyAgreement.PrivateKey(rawRepresentation: secret)
    }

    public static func remove() {
        SecItemDelete(query() as CFDictionary)
    }

    private static func query() -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecUseDataProtectionKeychain as String: true,
        ]
        if let group = SharedKeys.keychainGroup { query[kSecAttrAccessGroup as String] = group }
        return query
    }
}
