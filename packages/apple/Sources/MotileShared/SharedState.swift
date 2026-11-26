import Foundation

/// What the app leaves in the App Group for its extensions: what the agents do, for the widget,
/// and the badge, which the notification extension counts on from.
public enum SharedState {
    private static let workKey = "work"
    private static let badgeKey = "badge"
    private static let badgeThreadsKey = "badgeThreads"

    static var defaults: UserDefaults = UserDefaults(suiteName: SharedKeys.appGroup) ?? .standard

    public static var work: [ServerWork] {
        guard let data = defaults.data(forKey: workKey) else { return [] }
        return (try? Pushed.decoder.decode([ServerWork].self, from: data)) ?? []
    }

    /// Keeps the work; whether it changed, so the widget's timeline is reloaded only then.
    @discardableResult
    public static func setWork(_ work: [ServerWork]) -> Bool {
        guard work != self.work, let data = try? Pushed.encoder.encode(work) else { return false }
        defaults.set(data, forKey: workKey)
        return true
    }

    /// The core's count, as the app last heard it.
    public static var badge: Int {
        get { defaults.integer(forKey: badgeKey) }
        set { defaults.set(newValue, forKey: badgeKey) }
    }

    /// The threads alerted while the app was away, which the badge counts on top of `badge`.
    public static var badgeThreads: [String] {
        get { defaults.stringArray(forKey: badgeThreadsKey) ?? [] }
        set { defaults.set(newValue, forKey: badgeThreadsKey) }
    }

    /// The app heard the core's count: what was counted while it was away is in it now.
    public static func setBadge(_ count: Int) {
        badge = count
        badgeThreads = []
    }

    /// Counts the thread of an alert that came while the app was away; the badge to show.
    public static func badgeAfterAlert(threadId: String) -> Int {
        var threads = badgeThreads
        if !threads.contains(threadId) {
            threads.append(threadId)
            badgeThreads = threads
        }
        return badge + threads.count
    }
}
