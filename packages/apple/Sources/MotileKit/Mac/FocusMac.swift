#if os(macOS)
import AppKit

/// Whether the user is in the Mac app: it is in front, one of its windows has the keys and shows,
/// and the screen is awake, unlocked and theirs. And when they last used a keyboard, a mouse or a
/// trackpad.
final class FocusWatch {
    private var asleep = false
    private var locked = false
    private var switchedAway = false
    private var changed: () -> Void = {}

    var focused: Bool {
        guard NSApp.isActive, !asleep, !locked, !switchedAway, let window = NSApp.keyWindow else { return false }
        return window.occlusionState.contains(.visible)
    }

    var lastInput: Date? {
        guard let any = CGEventType(rawValue: ~0) else { return nil }
        return Date(timeIntervalSinceNow: -CGEventSource.secondsSinceLastEventType(.combinedSessionState, eventType: any))
    }

    func start(changed: @escaping () -> Void) {
        self.changed = changed
        let windows = [
            NSApplication.didBecomeActiveNotification, NSApplication.didResignActiveNotification,
            NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification, NSWindow.didChangeOcclusionStateNotification,
        ]
        for name in windows {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in self?.changed() }
        }
        let workspace = NSWorkspace.shared.notificationCenter
        on(workspace, NSWorkspace.screensDidSleepNotification) { $0.asleep = true }
        on(workspace, NSWorkspace.screensDidWakeNotification) { $0.asleep = false }
        on(workspace, NSWorkspace.sessionDidResignActiveNotification) { $0.switchedAway = true }
        on(workspace, NSWorkspace.sessionDidBecomeActiveNotification) { $0.switchedAway = false }
        let distributed = DistributedNotificationCenter.default()
        on(distributed, Notification.Name("com.apple.screenIsLocked")) { $0.locked = true }
        on(distributed, Notification.Name("com.apple.screenIsUnlocked")) { $0.locked = false }
    }

    private func on(_ center: NotificationCenter, _ name: Notification.Name, _ change: @escaping (FocusWatch) -> Void) {
        center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
            guard let self else { return }
            change(self)
            changed()
        }
    }
}
#endif
