#if os(iOS)
import ActivityKit
import MotileShared
import SwiftUI
import UIKit
import UserNotifications
import WidgetKit

/// MotileKit's own `Activity` is what a thread is doing.
private typealias LiveActivity = ActivityKit.Activity<AgentsAttributes>

/// The phone's side of the alerts: the system's permission, the push token and the Live Activity's
/// tokens the auth server pushes to, the badge, the widget's work, and opening a thread from a
/// notification or a widget.
@MainActor @Observable
final class PushNotifications: NSObject {
    private static let askedKey = "notifications.asked"

    /// What the system lets Motile show.
    private(set) var permission = UNAuthorizationStatus.notDetermined

    @ObservationIgnored private weak var store: AppStore?
    @ObservationIgnored private var deviceToken: String?
    @ObservationIgnored private var liveStartToken: String?
    /// The token of the Live Activity that runs, and that activity's id.
    @ObservationIgnored private var liveToken: (token: String, activity: String)?
    @ObservationIgnored private var watched = Set<String>()
    @ObservationIgnored private var sent: Registration?
    @ObservationIgnored private var keptSecret = false
    @ObservationIgnored private var pendingLink: ThreadLink?

    private struct Registration: Equatable {
        let token: String
        let liveStartToken: String?
        let liveToken: String?
    }

    var allowed: Bool {
        permission == .authorized || permission == .provisional || permission == .ephemeral
    }

    func attach(_ store: AppStore) {
        guard self.store == nil else { return }
        self.store = store
        store.alertPresenter = self
        watchLiveActivities()
        Task { await refreshPermission() }
        if let link = pendingLink { open(link) }
    }

    /// The account, the settings or the permission changed.
    func update() {
        guard let store, store.ready else { return }
        guard store.account.signedIn else { return forgetSecret() }
        keepSecret()
        askOnce()
        if !store.notificationSettings.live { endLiveActivities() }
        register()
    }

    /// Asks the system to let Motile notify; false when it won't.
    func requestPermission() async -> Bool {
        let granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        await refreshPermission()
        return granted
    }

    func refreshPermission() async {
        permission = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
        update()
    }

    func receivedDeviceToken(_ token: Data) {
        deviceToken = token.hex
        register()
    }

    func badgeChanged(_ count: Int) {
        SharedState.setBadge(count)
        UNUserNotificationCenter.current().setBadgeCount(count)
    }

    func workChanged(_ work: [ServerWork]) {
        guard SharedState.setWork(work) else { return }
        WidgetCenter.shared.reloadAllTimelines()
    }

    /// The alerts that came while the app was away are in the core's count once it is back.
    func wentToBackground() {
        SharedState.badgeThreads = []
    }

    func open(_ link: ThreadLink) {
        guard let store, store.ready else { return pendingLink = link }
        pendingLink = nil
        store.openThread(serverId: link.serverId, threadId: link.threadId)
    }

    /// The first launch after signing in asks, except in the dev app and the UI tests, whose
    /// system prompt would cover the client.
    private func askOnce() {
        guard !UserDefaults.standard.bool(forKey: Self.askedKey), permission == .notDetermined,
              store?.notificationSettings.mode != .off, ProcessInfo.processInfo.environment["MOTILE_DEMO_SIGN_IN"] == nil else { return }
        UserDefaults.standard.set(true, forKey: Self.askedKey)
        Task { _ = await requestPermission() }
    }

    /// The extensions open pushes with this device's key, which only the core has.
    private func keepSecret() {
        guard !keptSecret, let store else { return }
        keptSecret = true
        Task {
            guard let secret = await store.pushSecret(), PushSecret.store(base64: secret) else { return keptSecret = false }
        }
    }

    /// A new sign-in comes with a new key, which the extensions must not open pushes with the old one of.
    private func forgetSecret() {
        guard keptSecret else { return }
        keptSecret = false
        PushSecret.remove()
    }

    /// Registers with the auth server for what the settings want, and only when that changed.
    private func register() {
        guard let store, store.ready, store.account.signedIn else { return }
        let settings = store.notificationSettings
        let alerts = settings.mode != .off && allowed
        guard alerts || settings.live else {
            guard sent != nil else { return }
            sent = nil
            return store.unregisterPush()
        }
        UIApplication.shared.registerForRemoteNotifications()
        guard let deviceToken else { return }
        let registration = Registration(
            token: deviceToken,
            liveStartToken: settings.live ? liveStartToken : nil,
            liveToken: settings.live ? liveToken?.token : nil
        )
        guard registration != sent else { return }
        sent = registration
        store.registerPush(
            service: .apns(topic: Bundle.main.bundleIdentifier ?? "", sandbox: Self.sandbox), token: registration.token,
            liveStartToken: registration.liveStartToken, liveToken: registration.liveToken
        )
    }

    private func watchLiveActivities() {
        Task {
            for await token in LiveActivity.pushToStartTokenUpdates {
                liveStartToken = token.hex
                register()
            }
        }
        Task {
            for await activity in LiveActivity.activityUpdates { watch(activity) }
        }
        LiveActivity.activities.forEach(watch)
    }

    /// Sends the token of a Live Activity while it runs, and forgets it once it ended: the auth
    /// server starts the next one again.
    private func watch(_ activity: LiveActivity) {
        guard watched.insert(activity.id).inserted else { return }
        guard store?.notificationSettings.live != false else {
            Task { await activity.end(nil, dismissalPolicy: .immediate) }
            return
        }
        Task {
            for await token in activity.pushTokenUpdates {
                guard activity.activityState == .active || activity.activityState == .stale else { continue }
                liveToken = (token.hex, activity.id)
                register()
            }
        }
        Task {
            for await state in activity.activityStateUpdates where state == .ended || state == .dismissed {
                watched.remove(activity.id)
                guard liveToken?.activity == activity.id else { continue }
                liveToken = nil
                register()
            }
        }
    }

    private func endLiveActivities() {
        for activity in LiveActivity.activities {
            Task { await activity.end(nil, dismissalPolicy: .immediate) }
        }
    }

    /// Whether the pushes come from APNs' sandbox: in the simulator, and in a build whose profile
    /// is for development. The App Store's and TestFlight's builds carry no profile.
    private static let sandbox: Bool = {
        #if targetEnvironment(simulator)
        return true
        #else
        guard let url = Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision"),
              let signed = try? Data(contentsOf: url),
              let start = signed.range(of: Data("<?xml".utf8)),
              let end = signed.range(of: Data("</plist>".utf8), in: start.upperBound..<signed.endIndex),
              let profile = try? PropertyListSerialization.propertyList(from: signed[start.lowerBound..<end.upperBound], format: nil) as? [String: Any],
              let entitlements = profile["Entitlements"] as? [String: Any] else { return false }
        return entitlements["aps-environment"] as? String == "development"
        #endif
    }()
}

extension PushNotifications: AlertPresenting {
    /// A phone's pushes show its alerts, so the core gives it none to present.
    nonisolated func present(_ alert: AlertEvent) {}

    nonisolated func threadOpened(serverId: String, threadId: String) {
        UNUserNotificationCenter.current().getDeliveredNotifications { delivered in
            let shown = delivered.filter { $0.request.content.userInfo["thread"] as? String == threadId }
            UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: shown.map(\.request.identifier))
        }
    }
}

extension PushNotifications: UNUserNotificationCenterDelegate {
    /// In front, an alert shows unless its thread is the one on screen. UIKit takes the answers to
    /// both of these only on the main thread.
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        let thread = notification.request.content.userInfo["thread"] as? String
        DispatchQueue.main.async {
            let onScreen = MainActor.assumeIsolated { thread != nil && self.store?.selectedThread?.id == thread }
            completionHandler(onScreen ? [] : [.banner, .list, .sound])
        }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let info = response.notification.request.content.userInfo
        let (thread, server) = (info["thread"] as? String, info["server"] as? String)
        DispatchQueue.main.async {
            MainActor.assumeIsolated { self.openTapped(thread: thread, server: server) }
            completionHandler()
        }
    }

    private func openTapped(thread: String?, server: String?) {
        guard let thread, let server = server ?? store?.anyThread(thread)?.serverID else { return }
        open(ThreadLink(serverId: server, threadId: thread))
    }
}

/// Keeps the phone's alerts in step with the store.
struct PushNotificationsSync: ViewModifier {
    let push: PushNotifications
    let store: AppStore
    @Environment(\.scenePhase) private var phase

    func body(content: Content) -> some View {
        content
            .onChange(of: store.ready, initial: true) {
                guard store.ready else { return }
                push.attach(store)
                push.update()
            }
            .onChange(of: store.account.signedIn) { push.update() }
            .onChange(of: store.notificationSettings) { push.update() }
            .onChange(of: store.badgeCount, initial: true) { push.badgeChanged(store.badgeCount) }
            .onChange(of: store.work, initial: true) { push.workChanged(store.work) }
            .onChange(of: phase) {
                switch phase {
                case .background: push.wentToBackground()
                case .active: Task { await push.refreshPermission() }
                default: break
                }
            }
            .onOpenURL { url in
                guard let link = ThreadLink(url: url) else { return }
                push.open(link)
            }
    }
}

private extension Data {
    var hex: String { map { String(format: "%02x", $0) }.joined() }
}
#endif
