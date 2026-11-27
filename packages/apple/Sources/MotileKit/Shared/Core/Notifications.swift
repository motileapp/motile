import MotileShared
import SwiftUI

/// How alerts reach the user on this device.
enum NotificationMode: String, CaseIterable, Identifiable, Codable {
    case off, notifications, sound
    case notificationsAndSound = "notifications_and_sound"

    var id: String { rawValue }

    var label: String {
        switch self {
        case .off: "Off"
        case .notifications: "Notifications only"
        case .sound: "Sound only"
        case .notificationsAndSound: "Notifications with sound"
        }
    }

    /// Whether it posts the system's notifications, which the system has to allow.
    var notifies: Bool { self == .notifications || self == .notificationsAndSound }
}

/// Which alerts the user wants.
struct AlertChoices: Codable, Equatable {
    var finished = true
    var failed = true
    var approval = true
    /// Questions and plans.
    var question = true
    var limit = true

    /// The switches under "Alert me when an agent", in order.
    static let rows: [(title: String, choice: WritableKeyPath<AlertChoices, Bool>)] = [
        ("Finishes", \.finished),
        ("Fails", \.failed),
        ("Needs approval", \.approval),
        ("Asks a question or presents a plan", \.question),
        ("Reaches its usage limit", \.limit),
    ]
}

/// This device's notification settings, which the core keeps.
struct NotificationSettings: Codable, Equatable {
    var mode = NotificationMode.notificationsAndSound
    /// While Motile is in front on another thread, an alert shows inside it.
    var toasts = false
    var choices = AlertChoices()
    /// A phone shows what the agents do while they work.
    var live = false

    init() {}

    init(json: JSON) {
        guard let data = try? JSONSerialization.data(withJSONObject: json),
            let settings = try? JSONDecoder().decode(NotificationSettings.self, from: data)
        else { return }
        self = settings
    }

    var json: JSON {
        guard let data = try? JSONEncoder().encode(self), let json = try? JSONSerialization.jsonObject(with: data) as? JSON else { return [:] }
        return json
    }
}

/// What the settings say of notifications, on every client.
enum NotificationsText {
    static let mode = "Alerts when an agent finishes, fails, or needs your approval or an answer."
    static let refusedOnMac = "Allow notifications for Motile in System Settings."
    static let refusedOnPhone = "Allow notifications for Motile in Settings."
    static let toasts = "When you're looking at another thread."
    static let choices = "Alert me when an agent"
    static let live = "Show what your agents are doing on the Lock Screen while they work."
    static let widget = "Add the Agents widget to your Home Screen or Lock Screen to see them at a glance."
}

extension AlertKind {
    var symbol: Symbol {
        switch self {
        case .finished: .circleCheck
        case .failed, .limit: .circleAlert
        case .approval: .shieldQuestionMark
        case .question, .plan, .other: .messageCircleQuestionMark
        }
    }

    var color: Color {
        switch self {
        case .finished: .themeSuccess
        case .failed: .themeDestructive
        case .limit: .themeWarning
        case .approval, .question, .plan, .other: .themeForeground
        }
    }
}

/// How an alert shows, as the core decided from the settings and where the user is.
struct AlertShown: Equatable {
    /// A notification of the system.
    let system: Bool
    let sound: Bool
    /// A toast inside the client, which opens the thread.
    let toast: Bool
}

/// The core's `alert` event.
struct AlertEvent: Equatable {
    let alert: PushedAlert
    let serverName: String
    let shown: AlertShown

    init?(json: JSON) {
        guard let data = json.object("alert").flatMap({ try? JSONSerialization.data(withJSONObject: $0) }),
            let alert = try? Pushed.decoder.decode(PushedAlert.self, from: data)
        else { return nil }
        let shown = json.object("shown") ?? [:]
        self.alert = alert
        serverName = json.string("server_name")
        self.shown = AlertShown(system: shown.bool("system"), sound: shown.bool("sound"), toast: shown.bool("toast"))
    }
}

/// What shows the alerts the system shows on this platform. The store shows the toasts itself.
protocol AlertPresenting: AnyObject {
    func present(_ alert: AlertEvent)
    func threadOpened(serverId: String, threadId: String)
}

/// Where a phone's pushes come from.
enum PushService: Equatable {
    case apns(topic: String, sandbox: Bool)
    case fcm

    var json: JSON {
        switch self {
        case .apns(let topic, let sandbox): ["kind": "apns", "topic": topic, "sandbox": sandbox]
        case .fcm: ["kind": "fcm"]
        }
    }
}

extension ServerWork {
    /// The core's `work` event's servers.
    static func list(_ json: [JSON]) -> [ServerWork] {
        guard let data = try? JSONSerialization.data(withJSONObject: json) else { return [] }
        return (try? Pushed.decoder.decode([ServerWork].self, from: data)) ?? []
    }
}

extension AppStore {
    /// Has the auth server push the servers' alerts to this phone, sealed to this device's key.
    func registerPush(service: PushService, token: String, liveStartToken: String?, liveToken: String?) {
        var fields: JSON = ["service": service.json, "token": token]
        fields["live_start_token"] = liveStartToken
        fields["live_token"] = liveToken
        core.send("register_push", fields)
    }

    func unregisterPush() {
        core.send("unregister_push")
    }

    /// This device's key as an X25519 secret, in base64, for what opens pushes without the core.
    @MainActor
    func pushSecret() async -> String? {
        await withCheckedContinuation { continuation in
            core.send("push_secret") { result in
                continuation.resume(returning: try? result.get().optionalString("secret"))
            }
        }
    }
}
