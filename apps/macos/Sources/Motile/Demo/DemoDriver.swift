import AppKit

/// A scripted walk through the app, used by CI to take screenshots and to check that the window
/// stays responsive. Runs only when `MOTILE_DEMO=1`.
///
/// It expects an auth server that allows the dev login. Once signed in it writes the install
/// token to `MOTILE_DEMO_TOKEN_FILE`; `scripts/ci-demo.sh` picks it up and starts a server with it,
/// whose agent is `scripts/fake-agent`.
enum DemoDriver {
    private static var started = false

    static func startIfRequested(store: AppStore) {
        let environment = ProcessInfo.processInfo.environment
        guard environment["MOTILE_DEMO"] == "1", !started else { return }
        started = true
        let output = URL(fileURLWithPath: environment["MOTILE_DEMO_OUTPUT"] ?? NSTemporaryDirectory())
        try? FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
        Task { @MainActor in
            let demo = Demo(store: store, output: output, environment: environment)
            await demo.run()
            NSApp.terminate(nil)
        }
    }
}

@MainActor
private final class Demo {
    let store: AppStore
    let output: URL
    let environment: [String: String]
    var results: [String] = []

    init(store: AppStore, output: URL, environment: [String: String]) {
        self.store = store
        self.output = output
        self.environment = environment
    }

    private var window: NSWindow? {
        NSApp.windows.first { $0.isVisible && $0.canBecomeMain }
    }

    private func wait(_ seconds: Double) async {
        try? await Task.sleep(for: .seconds(seconds))
    }

    /// Waits until the condition holds, and records whether it did.
    @discardableResult
    private func expect(_ what: String, within seconds: Double = 30, _ condition: () -> Bool) async -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while !condition(), Date() < deadline {
            await wait(0.1)
        }
        let passed = condition()
        results.append("\(passed ? "PASS" : "FAIL") \(what)")
        return passed
    }

    private func shoot(_ name: String) async {
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
        await wait(0.7)
        guard let window else { return }
        capture(window, name)
    }

    private func capture(_ window: NSWindow, _ name: String) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        process.arguments = ["-x", "-o", "-l\(window.windowNumber)", output.appendingPathComponent("\(name).png").path]
        try? process.run()
        process.waitUntilExit()
    }

    /// The Settings window, opened from the app's menu as a person would.
    private func shootSettings(_ name: String) async {
        guard let menu = NSApp.mainMenu?.items.first?.submenu,
            let item = menu.items.firstIndex(where: { $0.keyEquivalent == "," })
        else { return }
        menu.performActionForItem(at: item)
        await wait(1)
        guard let settings = NSApp.windows.first(where: { $0.isVisible && $0.identifier?.rawValue.contains("Settings") == true }) else {
            results.append("FAIL the Settings window opens")
            return
        }
        capture(settings, name)
        settings.close()
    }

    /// The whole screen, to see the window's glass over what is behind it.
    private func shootScreen(_ name: String) async {
        await wait(0.5)
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        process.arguments = ["-x", output.appendingPathComponent("\(name).png").path]
        try? process.run()
        process.waitUntilExit()
    }

    private var turnEnded: Bool {
        guard let last = store.transcript.rows.last, case .turnEnd = last.kind else { return false }
        return !store.activity.running
    }

    private var turnEnds: Int {
        let ends = store.transcript.rows.filter { row in
            guard case .turnEnd = row.kind else { return false }
            return true
        }
        return ends.count
    }

    private func send(_ text: String) {
        store.draft = text
        store.send()
    }

    /// What the rows of the queued messages say, in order.
    private var queuedStatuses: [String] {
        store.transcript.rows.compactMap { row in
            guard case .queued(let message) = row.kind else { return nil }
            return message.status
        }
    }

    func run() async {
        await wait(2)
        window?.setFrame(NSRect(x: 60, y: 60, width: 1280, height: 840), display: true)
        await expect("an unlinked Mac is shown the sign-in screen") { store.ready && !store.account.signedIn }
        await shoot("01-sign-in")

        store.devSignIn(email: "demo@motile.app")
        let signedIn = await expect("signing in leads to the install command") { store.account.signedIn && store.enrollToken != nil }
        guard signedIn else { return finish() }
        await shoot("02-connect-server")

        // The install command's token goes to the script, which starts the server with it.
        if let file = environment["MOTILE_DEMO_TOKEN_FILE"], let command = store.enrollToken?.command {
            let token = command.split(separator: " ").last.map(String.init) ?? ""
            try? token.write(toFile: file, atomically: true, encoding: .utf8)
        }
        let connected = await expect("the server appears once the install command has run", within: 90) {
            store.servers.first?.state == .connected
        }
        guard connected else { return finish() }
        await shoot("03-add-project")

        store.addProject()
        await wait(1)
        await shoot("03-add-project-panel")
        store.closePanel()
        await wait(0.6)

        guard let server = store.servers.first else { return finish() }
        store.addProject(serverID: server.id, path: environment["MOTILE_DEMO_PROJECT"] ?? NSTemporaryDirectory())
        await expect("a folder on the server becomes a project") { store.project(store.selectedDraft?.projectID) != nil }
        await expect("the project's icon is fetched from the server") { store.project(store.selectedDraft?.projectID)?.iconPath != nil }
        store.draft = "Add a rate limiter to the API"
        await shoot("04-new-thread")

        store.openPanel(.projects)
        await wait(0.6)
        await shoot("04-panel-projects")
        store.closePanel()

        store.send()
        await expect("sending the first message opens a thread") { store.selectedThread != nil }
        await wait(1.2)
        await shoot("05-working")
        let streaming = StallMonitor()
        await expect("the turn runs to its end", within: 60) { turnEnded }
        results.append(responsive("a reply streams", streaming))
        await expect("the thread gets a generated title") { store.selectedThread?.title == "Add API Rate Limiting" }
        let first = store.selectedThread?.id ?? ""
        await shoot("06-thread")

        // The finished turn is folded. The fold opens into what the agent did, with the tool
        // calls that followed one another as one row each, and those open into the calls.
        let fold = store.transcript.rows.first { $0.kindName == "fold" }
        await toggle(fold, "the fold of a finished turn")
        scrollTranscript(to: 0)
        await shoot("06-unfolded")
        for group in store.transcript.rows.filter({ $0.kindName == "group" }) {
            await toggle(group, "a group of tool calls")
        }
        let kinds = store.transcript.rows.map(\.kindName)
        results.append("\(kinds.contains("code") && kinds.contains("tool") && kinds.contains("prose") ? "PASS" : "FAIL") the transcript has prose, code and tool calls — \(kinds.count) rows")

        // Opening tool calls shows what they did.
        let toolRows = store.transcript.rows.filter { row in
            guard case .tool(let tool) = row.kind else { return false }
            return tool.verb == "Edited" || tool.verb == "Ran"
        }
        for row in toolRows { transcriptView?.rowToggledExpansion(id: row.id) }
        scrollTranscript(to: 0.12)
        await shoot("06-tool-details")
        for row in toolRows { transcriptView?.rowToggledExpansion(id: row.id) }
        await toggle(fold, "the open fold")

        // A supervised agent asks before it changes or runs something, and the turn waits.
        store.startNewThread()
        store.setAccess(.supervised)
        send("Change greet.py to use an f-string, then run greet.py.")
        await expect("a supervised turn waits for approval before a tool call") {
            store.selectedThread?.needsApproval == true && store.activity.approvals.first?.title == "Edit"
        }
        await shoot("07-approval")
        if let edit = store.activity.approvals.first { store.answer(edit, allow: true) }
        await expect("an allowed tool call runs, and the next one asks") { store.activity.approvals.first?.title == "Bash" }
        if let bash = store.activity.approvals.first { store.answer(bash, allow: false) }
        await expect("a refused tool call is left out, and the turn ends") {
            turnEnded && store.selectedThread?.needsApproval == false
        }
        await shoot("07-answered")

        // A message sent while the agent works waits in the queue until the turn ends. It can be
        // taken back, or sent now: then the agent takes it in the turn that runs.
        store.startNewThread()
        store.setAccess(.supervised)
        send("Change greet.py to use an f-string, then run greet.py.")
        await expect("the turn waits before its first tool call") { store.activity.approvals.first?.title == "Edit" }
        send("Use single quotes")
        await expect("a message sent while the agent works waits at the end of the transcript") {
            queuedStatuses == ["Queued"] && store.transcript.rows.last?.kindName == "queued"
        }
        send("Never mind")
        await expect("the next one waits behind it") { queuedStatuses == ["Queued", "Queued"] }
        await shoot("07-queued")
        if let last = store.activity.queued.last { store.takeBack(queued: last.id) }
        await expect("a queued message that is taken back returns to the composer") {
            queuedStatuses.count == 1 && store.draft == "Never mind"
        }
        store.draft = ""
        if let kept = store.activity.queued.first { store.sendNow(queued: kept.id) }
        await expect("a message sent now is being given to the agent") { queuedStatuses == ["Sending…"] }
        if let edit = store.activity.approvals.first { store.answer(edit, allow: true) }
        await expect("the next tool call asks") { store.activity.approvals.first?.title == "Bash" }
        if let bash = store.activity.approvals.first { store.answer(bash, allow: true) }
        await expect("the agent takes a message sent now in the turn that runs") {
            turnEnded && turnEnds == 1 && queuedStatuses.isEmpty && store.transcript.rows.filter(\.isUser).count == 2
        }
        await shoot("07-queue-taken")

        // A question the agent asks is answered in place, and the turn goes on with the answer.
        store.startNewThread()
        send("Which color should the button be? Ask me.")
        await expect("a question the agent asks is shown with its options") {
            store.activity.approvals.first?.questions.first?.options.count == 2
        }
        await shoot("07-question")
        if let asked = store.activity.approvals.first, let question = asked.questions.first {
            store.answer(asked, allow: true, answers: [question.text: "Blue"])
        }
        await expect("the agent goes on with the answer") { turnEnded }

        // A plan is approved in place: the thread leaves plan mode and the agent carries it out.
        store.startNewThread()
        store.setAccess(.full)
        store.setPlan(true)
        send("Plan the hello function.")
        await expect("a finished plan waits to be implemented") { store.activity.approvals.first?.allowLabel == "Implement" }
        await shoot("07-plan")
        if let plan = store.activity.approvals.first { store.answer(plan, allow: true) }
        await expect("an approved plan is carried out, and the thread leaves plan mode") {
            turnEnded && store.selectedThread?.plan == false
        }

        // An agent that keeps watching after its turn: it takes a message meanwhile, and says by
        // itself what it saw.
        store.startNewThread()
        store.setAccess(.full)
        store.setPlan(false)
        send("Watch the deploy and tell me when it is healthy.")
        await expect("an agent that watches something is shown as monitoring") {
            store.selectedThread?.monitoring == true && store.activity.monitoring
        }
        await shoot("07-monitoring")
        send("How far is it?")
        await expect("a message sent to a monitoring agent is answered right away") { turnEnds == 2 && store.activity.monitoring }
        await expect("the agent reports what it watched and is done") { turnEnds == 3 && store.selectedThread?.busy == false }
        await shoot("07-monitored")

        // The branch under the composer opens the picker, and a branch made there is checked out
        // on the server.
        if let project = store.composerProject {
            store.showBranches(of: project)
        }
        await wait(1)
        await shoot("07-branches")
        store.showsBranches = false
        if let project = store.composerProject {
            store.switchBranch(of: project, to: "demo/strips", create: true) { _ in }
        }
        await expect("a branch made from the picker is checked out") { store.composerProject?.branch == "demo/strips" }

        // An image the agent shows is fetched from the server and drawn in the reply.
        store.startNewThread()
        send("Show the screenshot of the landing page.")
        await expect("an image the agent shows is a row of its reply") {
            turnEnded && store.transcript.rows.contains { $0.kindName == "media" }
        }
        await wait(1)
        store.refreshMediaStorage()
        await expect("the image is fetched from the server and kept on this Mac") { (store.mediaStorage?.used ?? 0) > 0 }
        await shoot("07-image")

        // What the agent left in the folder is committed from the top bar in one click, with a
        // message the server has written for it.
        await expect("the git button offers to commit what the agent changed") {
            store.project(store.selectedThread?.projectID)?.gitControl?.quick.action == "commit"
        }
        if let project = store.project(store.selectedThread?.projectID) {
            await shoot("07-git-button")
            store.runQuickGit(in: project)
            await expect("one click commits with a written message") {
                store.gitNotice?.title.hasPrefix("Committed") == true
                    && store.project(project.id)?.gitControl?.quick.action == nil
            }
            await shoot("07-committed")
            store.dismissGitNotice()
        }

        // A long reply full of code, to see that the window keeps up.
        store.startNewThread()
        store.setAccess(.full)
        send("Give me a long reply with a lot of code.")
        await wait(0.5)
        let longStreaming = StallMonitor()
        await scrollByHandWhileStreaming()
        await expect("a long reply arrives whole", within: 90) { turnEnded }
        results.append(responsive("a long reply streams", longStreaming))
        let scrolling = StallMonitor()
        await scrollTranscript()
        results.append(responsive("a long reply is scrolled", scrolling))
        scrollTranscript(to: 0)
        await shoot("08-code-in-a-list")
        scrollTranscript(to: 0.45)
        await shoot("08-long-reply")

        // A very long thread: built, then opened again from the cache and scrolled.
        store.startNewThread()
        send("Make a huge transcript.")
        await expect("a very long thread is built", within: 180) { turnEnded }
        let huge = store.selectedThread?.id ?? ""
        store.select(.thread(first))
        await wait(1)
        let opening = StallMonitor()
        let began = Date()
        store.select(.thread(huge))
        await expect("the very long thread opens again from the cache, folded") { turnEnded }
        await wait(0.3)
        await toggle(store.transcript.rows.first { $0.kindName == "fold" }, "the fold of a very long turn")
        let rowCount = store.transcript.rows.count
        let opened = Int(Date().timeIntervalSince(began) * 1000)
        results.append(responsive("a thread of \(rowCount) rows opens (in \(opened) ms)", opening))
        await wait(0.5)
        let hugeScrolling = StallMonitor()
        await scrollTranscript()
        results.append(responsive("a thread of \(rowCount) rows is scrolled", hugeScrolling))
        scrollTranscript(to: 0.5)
        await shoot("09-huge-thread")

        // What updates look like, without one happening.
        store.updater.show(.available("9.9.9"), latest: "9.9.9")
        await shoot("09-update-available")
        store.updater.show(.downloading("9.9.9", 0.42), latest: "9.9.9")
        await shoot("09-update-downloading")
        store.updater.show(.idle, latest: store.updater.current)

        store.openPanel(.commands)
        await wait(0.6)
        await shoot("09-panel-commands")
        store.openPanel(.threads)
        await wait(0.6)
        await shoot("09-panel-threads")
        store.closePanel()

        // Marking the first thread done moves it to the Done shelf.
        UserDefaults.standard.set(true, forKey: "sidebar.doneExpanded")
        store.select(.thread(first))
        await wait(0.5)
        store.setDone([first], done: true)
        await expect("a thread marked done is listed as done") { store.doneThreads.map(\.id) == [first] }
        await shoot("10-done")
        await shootScreen("10-screen-light")

        await shootSettings("10-settings")

        NSApp.appearance = NSAppearance(named: .darkAqua)
        await shootSettings("11-dark-settings")
        await shoot("11-dark-thread")
        await shootScreen("11-screen-dark")
        store.setDone([first], done: false)
        await expect("a thread marked undone is active again") { store.doneThreads.isEmpty }
        store.startNewThread()
        store.draft = "Why is the sync slow on large threads?\n\nLook at how the server answers `Open` first:\n- what it reads from SQLite\n- how many items it sends"
        store.attach([URL(fileURLWithPath: "/tmp/motile-demo/api/greet.py")])
        await expect("an attached file is on the server before its message is sent") { store.attachments.first?.state == .ready && store.canSend }
        await shoot("12-dark-new-thread")

        // A new thread that was written in stays in the sidebar as a draft until it is sent or discarded.
        store.select(.thread(first))
        await expect("a new thread that was written but not sent is listed as a draft") { store.listedDrafts.count == 1 }
        store.startNewThread()
        store.startNewThread()
        await expect("new threads with nothing written in them are not listed") { store.listedDrafts.count == 1 }
        await shoot("12-dark-draft")
        if let written = store.listedDrafts.last { store.select(.draft(written.id)) }
        await expect("the draft opens with what was written in it") { store.draft.hasPrefix("Why is the sync slow") && store.attachments.count == 1 }
        for listed in store.listedDrafts { store.discard(listed.draft) }
        await expect("discarded drafts are gone, and a thread is open instead") { store.listedDrafts.isEmpty && store.selectedThread != nil }

        // The smallest window with the widest sidebar: the sidebar gives way to the thread.
        UserDefaults.standard.set(420.0, forKey: "sidebar.width")
        store.select(.thread(first))
        window?.setFrame(NSRect(x: 60, y: 60, width: 780, height: 600), display: true)
        await wait(0.5)
        await expect("the thread fits in the smallest window") { transcriptFitsWindow }
        await shoot("13-smallest-window")
        finish()
    }

    private var transcriptFitsWindow: Bool {
        guard let transcript = transcriptView, let content = window?.contentView else { return false }
        let frame = transcript.convert(transcript.bounds, to: content)
        return frame.minX >= 0 && frame.maxX <= content.bounds.maxX + 0.5
    }

    private func responsive(_ what: String, _ monitor: StallMonitor) -> String {
        // A runner's virtual display can hold the main thread by itself, so only a real freeze
        // fails; the numbers are reported either way.
        let report = monitor.report()
        let verdict = report.longest < 1000 ? "PASS" : "FAIL"
        return "\(verdict) the window stays responsive while \(what) — longest stall \(report.longest) ms, \(report.slow) of \(report.samples) checks over 50 ms"
    }

    /// Clicks a group or a fold and waits for its rows to come or go.
    private func toggle(_ row: RowModel?, _ what: String) async {
        guard let row else {
            results.append("FAIL \(what) is in the transcript")
            return
        }
        let count = store.transcript.rows.count
        transcriptView?.toggleRow(id: row.id)
        await expect("clicking \(what) opens or closes it") { store.transcript.rows.count != count }
    }

    private var transcriptView: TranscriptView? {
        func find(in view: NSView) -> TranscriptView? {
            (view as? TranscriptView) ?? view.subviews.lazy.compactMap(find).first
        }
        return window?.contentView.flatMap(find)
    }

    private var transcriptScrollView: NSScrollView? {
        func scrollViews(in view: NSView) -> [NSScrollView] {
            ((view as? NSScrollView).map { [$0] } ?? []) + view.subviews.flatMap(scrollViews)
        }
        guard let content = window?.contentView else { return nil }
        return scrollViews(in: content).max {
            ($0.documentView?.frame.height ?? 0) < ($1.documentView?.frame.height ?? 0)
        }
    }

    /// Puts the transcript at a fraction of its height: 0 is the top, 1 the end.
    private func scrollTranscript(to fraction: Double) {
        guard let scrollView = transcriptScrollView, let document = scrollView.documentView else { return }
        let bottom = max(0, document.frame.height - scrollView.contentView.bounds.height)
        scrollView.contentView.scroll(to: NSPoint(x: 0, y: bottom * fraction))
        scrollView.reflectScrolledClipView(scrollView.contentView)
    }

    /// Scrolls the transcript to the top and back down in steps, as a person reading it would.
    private func scrollTranscript() async {
        for step in [1.0, 0.8, 0.6, 0.4, 0.2, 0.0, 0.25, 0.5, 0.75, 1.0] {
            scrollTranscript(to: step)
            await wait(0.15)
        }
    }

    /// Scrolls as a hand on a trackpad does while a reply streams: up, back down to just above
    /// the end, held there, and let go.
    private func scrollByHandWhileStreaming() async {
        guard let scrollView = transcriptScrollView, let document = scrollView.documentView else { return }
        let clip = scrollView.contentView
        let end = { max(0, document.frame.height - clip.bounds.height) }
        var waited = 0.0
        while end() < 300, !turnEnded, waited < 30 {
            await wait(0.1)
            waited += 0.1
        }
        NotificationCenter.default.post(name: NSScrollView.willStartLiveScrollNotification, object: scrollView)
        clip.scroll(to: NSPoint(x: 0, y: max(0, end() - 200)))
        let held = max(0, end() - 10)
        clip.scroll(to: NSPoint(x: 0, y: held))
        await wait(1)
        let stayed = abs(clip.bounds.minY - held) < 1
        results.append("\(stayed ? "PASS" : "FAIL") the transcript stays where it is scrolled to while a reply streams under it")
        NotificationCenter.default.post(name: NSScrollView.didEndLiveScrollNotification, object: scrollView)
        await expect("let go near the end, the transcript follows the reply again", within: 5) { clip.bounds.minY > held + 5 }
    }

    private func finish() {
        let report = results.joined(separator: "\n") + "\n"
        print(report)
        try? report.write(to: output.appendingPathComponent("checks.txt"), atomically: true, encoding: .utf8)
    }
}

extension RowModel {
    var kindName: String {
        switch kind {
        case .user: "user"
        case .prose: "prose"
        case .code: "code"
        case .tool: "tool"
        case .thinking: "thinking"
        case .media: "media"
        case .group: "group"
        case .fold: "fold"
        case .error: "error"
        case .turnEnd: "turn_end"
        case .queued: "queued"
        }
    }
}

/// Times how long the main thread takes to get to a piece of work, over and over. A long wait is
/// what the user sees as the window freezing.
final class StallMonitor: @unchecked Sendable {
    struct Report {
        let longest: Int
        /// How many of the waits were longer than three frames.
        let slow: Int
        let samples: Int
    }

    private let lock = NSLock()
    private var waits: [Double] = []
    private var stopped = false

    init() {
        Thread.detachNewThread { [weak self] in
            while let self, !self.isStopped {
                let asked = Date()
                let ran = DispatchSemaphore(value: 0)
                DispatchQueue.main.async { ran.signal() }
                ran.wait()
                self.record(Date().timeIntervalSince(asked))
                Thread.sleep(forTimeInterval: 0.01)
            }
        }
    }

    private var isStopped: Bool {
        lock.lock()
        defer { lock.unlock() }
        return stopped
    }

    private func record(_ wait: TimeInterval) {
        lock.lock()
        waits.append(wait)
        lock.unlock()
    }

    /// What was measured so far. Asking ends the measuring.
    func report() -> Report {
        lock.lock()
        defer { lock.unlock() }
        stopped = true
        return Report(
            longest: Int((waits.max() ?? 0) * 1000),
            slow: waits.filter { $0 > 0.05 }.count,
            samples: waits.count
        )
    }
}
