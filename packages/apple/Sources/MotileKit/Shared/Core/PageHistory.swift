import Foundation

/// What fills the client in place of the others: a thread or a draft, an agent shown in its
/// thread's place, a section of the settings or the usage. The panels beside them aren't pages.
enum Page: Hashable {
    case selection(Selection)
    case agent(threadID: String, agentID: String)
    case settings(SettingsSection)
    case usage
}

/// The pages the client went through, for Go Back and Go Forward.
struct PageHistory {
    private static let limit = 50

    private var back: [Page] = []
    private var forward: [Page] = []
    private(set) var current: Page?

    /// The client is on `page`; the one it left goes behind it, and nothing is ahead any more.
    mutating func visit(_ page: Page) {
        guard let current, current != page else {
            self.current = page
            return
        }
        back.append(current)
        if back.count > Self.limit { back.removeFirst() }
        forward = []
        self.current = page
    }

    func canStep(back isBack: Bool, exists: (Page) -> Bool) -> Bool {
        (isBack ? back : forward).contains(where: exists)
    }

    /// The nearest page behind or ahead that still exists, which becomes the current one. The
    /// ones on the way that don't are dropped.
    mutating func step(back isBack: Bool, exists: (Page) -> Bool) -> Page? {
        while let page = isBack ? back.popLast() : forward.popLast() {
            guard exists(page) else { continue }
            if let current {
                if isBack { forward.append(current) } else { back.append(current) }
            }
            current = page
            return page
        }
        return nil
    }

    /// Where the client ended up after a step, kept without moving the rest.
    mutating func land(on page: Page) {
        current = page
    }
}

extension AppStore {
    var page: Page {
        if let settings { return .settings(settings) }
        if showsUsage { return .usage }
        if let agentID = rightPanel.shownAgent, case .thread(let threadID) = selection {
            return .agent(threadID: threadID, agentID: agentID)
        }
        return .selection(selection)
    }

    /// Records the page once what changed with it has, so that the steps on the way aren't pages.
    func pageChanged() {
        guard !pageCheckPending else { return }
        pageCheckPending = true
        DispatchQueue.main.async { [weak self] in self?.recordPage() }
    }

    func canGo(back: Bool) -> Bool {
        history.canStep(back: back, exists: exists)
    }

    /// Goes back or forward to the nearest page that is still there. Whether there was one.
    func go(back: Bool) -> Bool {
        recordPage()
        guard let page = history.step(back: back, exists: exists) else { return false }
        show(page)
        let landed = self.page
        if landed != page { history.land(on: landed) }
        return true
    }

    private func recordPage() {
        pageCheckPending = false
        let page = page
        guard history.current != page else { return }
        history.visit(page)
    }

    private func exists(_ page: Page) -> Bool {
        switch page {
        case .selection(.thread(let id)), .agent(let id, _): anyThread(id) != nil
        case .selection(.draft(let id)): threadDrafts.contains { $0.id == id }
        case .settings, .usage: account.signedIn
        }
    }

    private func show(_ page: Page) {
        switch page {
        case .settings(let section): openSettings(section)
        case .usage: openUsage()
        case .selection(let selection):
            closeRoute()
            show(selection)
            rightPanel.showAgents()
        case .agent(let threadID, let agentID):
            closeRoute()
            show(.thread(threadID))
            rightPanel.showAgent(agentID)
        }
    }
}
