import Foundation
import Observation

/// Tells the core whether the user is in the client and which project of a server it shows, and,
/// while they are, that they still use it, for the user's time in the usage's Activity tab. The
/// core does the counting. Used on the main thread alone.
final class Engagement: @unchecked Sendable {
    private struct Shown: Equatable {
        let focused: Bool
        let place: Place?
    }

    struct Place: Equatable {
        let serverID: String
        let projectID: String
    }

    private static let inputInterval: TimeInterval = 5

    private weak var store: AppStore?
    private let focus = FocusWatch()
    private var sent: Shown?
    private var updatePending = false
    private var timer: Timer?
    /// When the user last did something, as last sent.
    private var inputSent = Date.distantPast

    func start(_ store: AppStore) {
        self.store = store
        focus.start { [weak self] in self?.scheduleUpdate() }
        followPlace()
        update()
    }

    private func followPlace() {
        withObservationTracking {
            _ = store?.shownPlace
        } onChange: { [weak self] in
            DispatchQueue.main.async {
                self?.scheduleUpdate()
                self?.followPlace()
            }
        }
    }

    /// Several changes at once, like one window giving the keys to another, are one update.
    private func scheduleUpdate() {
        guard !updatePending else { return }
        updatePending = true
        DispatchQueue.main.async { [weak self] in self?.update() }
    }

    private func update() {
        updatePending = false
        guard let store else { return }
        let shown = Shown(focused: focus.focused, place: store.shownPlace)
        if shown != sent {
            if shown.focused && sent?.focused != true { inputSent = Date() }
            sent = shown
            var fields: JSON = ["focused": shown.focused]
            if let place = shown.place {
                fields["server_id"] = place.serverID
                fields["project_id"] = place.projectID
            }
            store.core.send("focus", fields)
        }
        shown.focused ? startTimer() : stopTimer()
    }

    private func startTimer() {
        guard timer == nil else { return }
        let timer = Timer(timeInterval: Self.inputInterval, repeats: true) { [weak self] _ in self?.sendInput() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func stopTimer() {
        timer?.invalidate()
        timer = nil
    }

    private func sendInput() {
        guard sent?.focused == true, let latest = focus.lastInput, latest.timeIntervalSince(inputSent) > 0.5 else { return }
        inputSent = latest
        store?.core.send("input", ["idle_secs": max(Date().timeIntervalSince(latest), 0)])
    }
}

extension AppStore {
    /// The project of a server on screen: the open thread's, or the open draft's. None while the
    /// settings or the usage cover them.
    var shownPlace: Engagement.Place? {
        guard settings == nil, !showsUsage else { return nil }
        if let thread = selectedThread { return Engagement.Place(serverID: thread.serverID, projectID: thread.projectID) }
        guard let project = project(selectedDraft?.projectID) else { return nil }
        return Engagement.Place(serverID: project.serverID, projectID: project.id)
    }
}
