import Foundation

/// Where the app and its extensions meet: the App Group and the keychain group they all have.
public enum SharedKeys {
    public static let appGroup = "group.app.motile.motile"

    /// `<AppIdentifierPrefix>app.motile.motile.shared`, the prefix from the target's Info.plist.
    /// Nil where the bundle has none, as in the tests.
    public static var keychainGroup: String? {
        guard let prefix = Bundle.main.object(forInfoDictionaryKey: "AppIdentifierPrefix") as? String,
              !prefix.contains("$(") else { return nil }
        return prefix + "app.motile.motile.shared"
    }
}
