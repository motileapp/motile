#if os(macOS)
import Foundation
import Observation

/// What the menu bar shows of the store. The menus are built again whenever what they read
/// changes, which is costly, so they read this, which changes only when what they show does.
struct MenuState: Equatable {
    var panelOpen = false
    var panelMaximized = false
    var panelCanMaximize = false
    var panelUnavailable = true
    var panelRepository = false
    var panelSwitchable = false
    var pullRequestsUnavailable = true
    var pullRequestsExtended = false
    var linear = false
    var linearUnavailable = true
    var ricrac = false
    var ricracUnavailable = true
    /// The title of the item that starts a thread in the composer's project, when there is one.
    var newThreadHere: String?
    var noProjects = true
    var signedIn = false
    var threadOpen = false
    var threadDone = false
    var removedServer = false
    var busy = false
    var hasJumps = false
    var canGoBack = false
    var canGoForward = false
    var hasServers = false

    @MainActor init() {}

    @MainActor init(_ store: AppStore) {
        let panel = store.rightPanel
        panelOpen = panel.isOpen
        panelMaximized = panel.isMaximized
        panelCanMaximize = panel.canMaximize
        panelUnavailable = store.panelUnavailable != nil
        panelRepository = store.panelTarget?.repository == true
        panelSwitchable = panel.isOpen && panel.tabs.tabs.count > 1
        pullRequestsUnavailable = store.pullRequestsUnavailable != nil
        pullRequestsExtended = store.pullRequestsExtended
        linear = store.isOn(.linear)
        linearUnavailable = store.linearUnavailable != nil
        ricrac = store.isOn(.ricrac)
        ricracUnavailable = store.ricracUnavailable != nil
        newThreadHere = store.composerProject.map { $0.noProject ? "New Thread Without a Project" : "New Thread in “\($0.name)”" }
        noProjects = store.noProjects.isEmpty
        signedIn = store.account.signedIn
        threadOpen = store.selectedThread != nil
        threadDone = store.selectedThread?.isDone == true
        removedServer = store.openThreadRemovedServer != nil
        busy = store.activity.busy
        hasJumps = !store.jumpThreads.isEmpty
        canGoBack = store.canGo(back: true)
        canGoForward = store.canGo(back: false)
        hasServers = !store.servers.isEmpty
    }
}

/// Keeps `state` as the store has it, and assigns it only when it changes.
@MainActor @Observable
final class MenuModel {
    private(set) var state = MenuState()
    @ObservationIgnored private weak var store: AppStore?

    func follow(_ store: AppStore) {
        self.store = store
        update()
    }

    private func update() {
        guard let store else { return }
        let new = withObservationTracking { MenuState(store) } onChange: { [weak self] in
            Task { @MainActor in self?.update() }
        }
        if new != state { state = new }
    }
}
#endif
