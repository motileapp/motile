#if os(macOS)
import AppKit
import UserNotifications

/// Shows the alerts the core gives as a Mac does: a notification for each thread, the system's
/// sound, and the Dock's badge. Clicking a notification brings the window forward on its thread.
/// Used on the main thread alone.
@Observable
final class MacAlerts: NSObject, AlertPresenting, UNUserNotificationCenterDelegate, @unchecked Sendable {
    static let shared = MacAlerts()
    private static let askedKey = "notifications.asked"

    /// The user refused Motile's notifications in System Settings.
    private(set) var refused = false
    @ObservationIgnored private weak var store: AppStore?
    /// The notification clicked before the store started, which launched Motile.
    @ObservationIgnored private var clicked: (serverID: String, threadID: String)?

    /// Only an app bundle has notifications; the bare executable the tests run has none.
    private var center: UNUserNotificationCenter? {
        Bundle.main.bundleURL.pathExtension == "app" ? .current() : nil
    }

    /// Before the launch ends, so that the click that launched Motile comes here.
    func install() {
        center?.delegate = self
    }

    func start(_ store: AppStore) {
        self.store = store
        store.alertPresenter = self
        follow()
        readPermission()
        NotificationCenter.default.addObserver(forName: Platform.becameActive, object: nil, queue: .main) { [weak self] _ in
            self?.readPermission()
            self?.center?.removeAllDeliveredNotifications()
        }
        if let clicked { open(clicked) }
    }

    func present(_ event: AlertEvent) {
        guard event.shown.system, let center, !refused else {
            if event.shown.sound { NSSound.beep() }
            return
        }
        let alert = event.alert
        let content = UNMutableNotificationContent()
        content.title = alert.threadTitle
        content.subtitle = alert.project
        content.body = alert.body
        content.threadIdentifier = alert.threadId
        content.userInfo = ["server_id": alert.serverId, "thread_id": alert.threadId]
        if event.shown.sound { content.sound = .default }
        center.add(UNNotificationRequest(identifier: alert.threadId, content: content, trigger: nil))
    }

    func threadOpened(serverId: String, threadId: String) {
        center?.removeDeliveredNotifications(withIdentifiers: [threadId])
    }

    /// Sets the mode, asking the system first when it notifies. A refusal keeps the mode, and the
    /// settings say where to allow it.
    func choose(_ mode: NotificationMode) {
        guard let store else { return }
        var settings = store.notificationSettings
        settings.mode = mode
        guard mode.notifies else { return store.setNotificationSettings(settings) }
        askPermission { store.setNotificationSettings(settings) }
    }

    func openSystemSettings() {
        let id = Bundle.main.bundleIdentifier ?? "app.motile.motile"
        guard let url = URL(string: "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=\(id)") else { return }
        NSWorkspace.shared.open(url)
    }

    // MARK: The system

    func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .list, .sound])
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        defer { completionHandler() }
        let info = response.notification.request.content.userInfo
        guard let serverID = info["server_id"] as? String, let threadID = info["thread_id"] as? String else { return }
        DispatchQueue.main.async { self.open((serverID, threadID)) }
    }

    private func open(_ thread: (serverID: String, threadID: String)) {
        guard let store else {
            clicked = thread
            return
        }
        clicked = nil
        NSApp.activate(ignoringOtherApps: true)
        if let window = NSApp.windows.first(where: \.canBecomeMain) {
            if window.isMiniaturized { window.deminiaturize(nil) }
            window.makeKeyAndOrderFront(nil)
        }
        store.openThread(serverId: thread.serverID, threadId: thread.threadID)
    }

    /// Keeps the Dock's badge on the store's count, and asks for permission once signed in.
    private func follow() {
        guard let store else { return }
        withObservationTracking {
            let count = store.badgeCount
            NSApp.dockTile.badgeLabel = count > 0 ? "\(count)" : nil
            if store.account.signedIn { askOnce(store.notificationSettings.mode) }
        } onChange: { [weak self] in
            DispatchQueue.main.async { self?.follow() }
        }
    }

    /// The first time a signed-in Motile runs with a mode that notifies, which it does by default.
    private func askOnce(_ mode: NotificationMode) {
        let defaults = UserDefaults.standard
        guard mode.notifies, !defaults.bool(forKey: Self.askedKey), ProcessInfo.processInfo.environment["MOTILE_DEMO"] != "1" else { return }
        defaults.set(true, forKey: Self.askedKey)
        askPermission {}
    }

    private func askPermission(then done: @escaping () -> Void) {
        guard let center else { return done() }
        center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, _ in
            DispatchQueue.main.async {
                self.refused = !granted
                done()
            }
        }
    }

    private func readPermission() {
        center?.getNotificationSettings { settings in
            let refused = settings.authorizationStatus == .denied
            DispatchQueue.main.async {
                guard self.refused != refused else { return }
                self.refused = refused
            }
        }
    }
}
#endif
