import SwiftUI

/// What the keyboard shortcuts' commands do, and what their conditions ask about the client.
extension AppStore {
    /// The names that hold now for the rules' conditions, with what has the keys: the composer, or
    /// another field to type in.
    func shortcutContext(composerFocus: Bool, editableFocus: Bool) -> Set<String> {
        var context: Set<String> = []
        if composerFocus { context.insert("composerFocus") }
        if editableFocus { context.insert("editableFocus") }
        if selectedThread != nil {
            context.insert("threadOpen")
            if activity.running { context.insert("turnRunning") }
        }
        if sidePanel.isOpen { context.insert("rightPanelOpen") }
        if settings != nil { context.insert("settingsOpen") }
        if showsUsage { context.insert("usagePageOpen") }
        return context
    }

    /// The threads ⌘1 to ⌘9 and the next and previous thread go through: the sidebar's, as it
    /// lists them.
    var jumpThreads: [ThreadInfo] {
        searched(activeThreads, for: threadSearch)
    }

    /// The keys that open the thread at `index` among `jumpThreads`, drawn on its row while they are held.
    func jumpHint(at index: Int) -> String? {
        guard showsJumpHints, index < 9 else { return nil }
        return shortcuts.label("thread.jump.\(index + 1)")
    }

    /// Runs the command. Whether there was anything for it to do; without, the keys go on to
    /// whatever else takes them.
    func perform(_ command: String) -> Bool {
        if let place = command.wholeMatch(of: #/thread\.jump\.([1-9])/#).flatMap({ Int($0.1) }) {
            return showThread(at: place - 1)
        }
        if let action = usageCommands[command] {
            guard showsUsage else { return false }
            action()
            return true
        }
        switch command {
        case "commandPalette.toggle": return togglePanel(.commands)
        case "threadPicker.toggle": return togglePanel(.threads)
        case "settings.open":
            guard account.signedIn else { return false }
            openSettings()
        case "usage.open":
            guard account.signedIn else { return false }
            openUsage()
        case "appearance.cycle":
            let all = Appearance.allCases
            let current = Appearance(rawValue: UserDefaults.standard.string(forKey: "appearance") ?? "") ?? .system
            let next = all[((all.firstIndex(of: current) ?? 0) + 1) % all.count]
            UserDefaults.standard.set(next.rawValue, forKey: "appearance")
        case "chat.new":
            guard account.signedIn, !servers.isEmpty else { return false }
            newThread()
        case "chat.newLocal":
            guard let project = composerProject else { return false }
            startNewThread(in: project)
        case "chat.newWithoutProject":
            guard let project = noProjects.first(where: { $0.serverID == composerServer?.id }) ?? noProjects.first else { return false }
            startNewThread(in: project)
        case "thread.previous": return showThread(offset: -1)
        case "thread.next": return showThread(offset: 1)
        case "thread.done":
            guard selectedThread != nil else { return false }
            toggleDone()
        case "thread.undo":
            guard undo != nil else { return false }
            performUndo()
        case "thread.stop":
            guard activity.busy else { return false }
            stop()
        case "thread.steerQueuedMessage":
            guard let queued = activity.queued.first else { return false }
            sendNow(queued: queued.id)
        case "thread.editQueuedMessage":
            guard let queued = activity.queued.last else { return false }
            takeBack(queued: queued.id)
        case "pullRequest.copyLink":
            guard let url = shownPullRequest?.url else { return false }
            Platform.copy(url.absoluteString)
        case "pullRequest.copyNumber":
            guard let number = shownPullRequest?.number else { return false }
            Platform.copy("#\(number)")
        case "composer.sendAlternate":
            guard selectedThread != nil, activity.running, canSend else { return false }
            send(alternate: true)
        case "composer.project":
            guard selectedDraft != nil else { return false }
            openPanel(.draftProject)
        case "composer.branch":
            guard let project = composerProject, canSwitchBranches(of: project) else { return false }
            showBranches(of: project)
        case "rightPanel.toggle": sidePanel.isOpen.toggle()
        case "rightPanel.toggleMaximized":
            guard sidePanel.canMaximize else { return false }
            sidePanel.toggleMaximized()
        case "rightPanel.new":
            guard panelUnavailable == nil else { return false }
            sidePanel.openBlank()
        case "rightPanel.nextTab", "rightPanel.previousTab":
            guard sidePanel.isOpen, sidePanel.tabs.tabs.count > 1 else { return false }
            sidePanel.activate(offset: command == "rightPanel.nextTab" ? 1 : -1)
        case "rightPanel.diff":
            guard panelUnavailable == nil, panelTarget?.repository == true else { return false }
            sidePanel.showDiff()
        case "rightPanel.files", "rightPanel.agents":
            guard panelUnavailable == nil else { return false }
            sidePanel.open(command == "rightPanel.files" ? .files : .agents)
        case "rightPanel.pullRequest", "rightPanel.pullRequests":
            let all = command == "rightPanel.pullRequests"
            guard panelUnavailable == nil, pullRequestsUnavailable == nil, !all || pullRequestsExtended else { return false }
            sidePanel.open(all ? .pullRequests : .pullRequest)
        case "rightPanel.linear":
            guard panelUnavailable == nil, linearUnavailable == nil else { return false }
            sidePanel.open(.linear)
        default: return performOnThisSystem(command)
        }
        return true
    }

    private var usageCommands: [String: () -> Void] {
        [
            "usage.cost": { self.usage.tab = .cost },
            "usage.tokens": { self.usage.tab = .tokens },
            "usage.limits": { self.usage.tab = .limits },
            "usage.period.day": { self.usage.period = .day },
            "usage.period.week": { self.usage.period = .week },
            "usage.period.month": { self.usage.period = .month },
            "usage.period.quarter": { self.usage.period = .quarter },
        ]
    }

    /// Opens the command panel on the page, or closes it when it is already there.
    private func togglePanel(_ page: PanelPage) -> Bool {
        guard panel != page else {
            closePanel()
            return true
        }
        guard account.signedIn, !servers.isEmpty else { return false }
        openPanel(page)
        return true
    }

    private func showThread(at index: Int) -> Bool {
        let threads = jumpThreads
        guard threads.indices.contains(index) else { return false }
        show(.thread(threads[index].id))
        return true
    }

    /// Opens the thread `offset` places from the open one in the sidebar. From a draft, the
    /// next is the first and the previous the last.
    private func showThread(offset: Int) -> Bool {
        let threads = jumpThreads
        guard !threads.isEmpty else { return false }
        guard let open = selectedThread, let index = threads.firstIndex(where: { $0.id == open.id }) else {
            return showThread(at: offset > 0 ? 0 : threads.count - 1)
        }
        return showThread(at: min(max(index + offset, 0), threads.count - 1))
    }

    /// The pull request the side panel shows, while its tab is the one in front.
    private var shownPullRequest: PullRequestPage? {
        guard sidePanel.isOpen else { return nil }
        switch sidePanel.tabs.active {
        case .pullRequest, .pullRequestNumber: return sidePanel.pullRequest.value
        default: return nil
        }
    }
}

/// How a key being chosen reads, and what is wrong with it.
struct ShortcutCheck: Equatable {
    var caps: [String] = []
    var conflicts: [String] = []
    /// The condition's names the client doesn't know, which are never true.
    var unknown: [String] = []
    var whenError: String?

    init() {}

    init(json: JSON) {
        caps = json.strings("caps")
        conflicts = json.strings("conflicts")
        unknown = json.strings("unknown")
        whenError = json.optionalString("when_error")
    }
}

/// Changing the keyboard shortcuts, from the settings.
extension AppStore {
    /// Gives the command the key, in place of the rule of `row` when it has one.
    func setShortcut(_ command: String, key: String, when: String?, replacing row: ShortcutRow? = nil) {
        var fields: JSON = ["command": command, "key": key]
        if let when, !when.trimmingCharacters(in: .whitespaces).isEmpty { fields["when"] = when }
        if let row, !row.key.isEmpty { fields["replace"] = row.rule }
        changeShortcuts("set_keybinding", fields)
    }

    func removeShortcut(_ row: ShortcutRow) {
        changeShortcuts("remove_keybinding", ["rule": row.rule])
    }

    func resetShortcut(_ command: String) {
        changeShortcuts("reset_keybinding", ["command": command])
    }

    /// Reads the key and the condition, as the rule of `row` if it has one, and says what they clash with.
    func checkShortcut(key: String, when: String?, row: ShortcutRow?, done: @escaping (ShortcutCheck) -> Void) {
        var fields: JSON = ["key": key]
        if let when { fields["when"] = when }
        if let row, !row.key.isEmpty { fields["row"] = row.rule }
        core.send("check_keybinding", fields) { result in
            guard case .success(let value) = result else { return }
            done(ShortcutCheck(json: value))
        }
    }

    /// Opens `keybindings.json` in the app that edits JSON here.
    func openShortcutsFile() {
        core.send("keybindings_file") { [weak self] result in
            switch result {
            case .success(let value): Platform.open(URL(fileURLWithPath: value.string("path")))
            case .failure(let error): self?.errorMessage = error.message
            }
        }
    }

    private func changeShortcuts(_ type: String, _ fields: JSON) {
        core.send(type, fields) { [weak self] result in
            guard case .failure(let error) = result else { return }
            self?.errorMessage = error.message
        }
    }
}
