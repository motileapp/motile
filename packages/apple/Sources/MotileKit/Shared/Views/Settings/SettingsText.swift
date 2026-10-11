import Foundation

/// What the settings say of a server, an account and the images kept here, on either client.
extension Server {
    var settingsDescription: String {
        guard state == .connected else { return stateLabel }
        return "Connected · version \(version) · \(installedAgents)"
    }

    var stateLabel: String {
        switch state {
        case .connected: "Connected"
        case .connecting: "Connecting"
        case .disconnected: "Offline"
        case .refused: "This server no longer accepts this \(Platform.device)"
        }
    }

    var installedAgents: String {
        let agents = agents.sorted { $0.key.rawValue < $1.key.rawValue }.map { "\($0.key.name) \($0.value)" }
        return agents.isEmpty ? "no agent installed" : agents.joined(separator: ", ")
    }

    /// Why its accounts can't be shown or changed, if they can't.
    var accountsUnavailable: String? {
        guard state == .connected else { return settingsDescription }
        guard switchesAccounts else { return "Update it to give its agents more accounts" }
        return nil
    }

    var textModelName: String {
        models.first { $0.id == textModel }?.name ?? "Automatic"
    }
}

extension AgentAccount {
    var settingsDescription: String {
        let signedIn = email.map { [$0, plan].compactMap { $0 }.joined(separator: " · ") }
        let who = signedIn ?? (variables.isEmpty ? "Not signed in" : "Signs in with its variables")
        return folder.isEmpty ? who : "\(who) · \(folder)"
    }
}

extension AppStore {
    /// The servers keep every image and video; the ones kept here only make threads open with them.
    var mediaStorageDescription: String {
        guard mediaStorage != nil else { return "Kept on this \(Platform.device) so threads open with them" }
        return "\(mediaStorageUsed) on this \(Platform.device). Your servers keep them all."
    }

    var mediaStorageUsed: String {
        guard let storage = mediaStorage else { return "" }
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        formatter.allowsNonnumericFormatting = false
        return "\(formatter.string(fromByteCount: storage.used)) of \(formatter.string(fromByteCount: storage.limit))"
    }

    static func steersDescription(_ steers: Bool) -> String {
        steers ? "The agent reads it at once, in the turn that runs" : "Waits for the turn to end and starts the next one"
    }
}
