import AppKit

/// A scripted walk through the app, used by CI to take screenshots and to check that the window
/// stays responsive. Runs only when `MOTILE_DEMO=1`.
///
/// It expects an auth server that allows the dev login. Once signed in it writes the install
/// token to `MOTILE_DEMO_TOKEN_FILE`; `scripts/ci-demo.sh` picks it up and starts a host with it,
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
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        process.arguments = ["-x", "-o", "-l\(window.windowNumber)", output.appendingPathComponent("\(name).png").path]
        try? process.run()
        process.waitUntilExit()
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

    private func send(_ text: String) {
        store.draft = text
        store.send()
    }

    func run() async {
        await wait(2)
        window?.setFrame(NSRect(x: 60, y: 60, width: 1280, height: 840), display: true)
        await expect("an unlinked Mac is shown the sign-in screen") { store.ready && !store.account.signedIn }
        await shoot("01-sign-in")

        store.devSignIn(email: "demo@motile.app")
        let signedIn = await expect("signing in leads to the install command") { store.account.signedIn && store.enrollToken != nil }
        guard signedIn else { return finish() }
        await shoot("02-connect-host")

        // The install command's token goes to the script, which starts the host with it.
        if let file = environment["MOTILE_DEMO_TOKEN_FILE"], let command = store.enrollToken?.command {
            let token = command.split(separator: " ").last.map(String.init) ?? ""
            try? token.write(toFile: file, atomically: true, encoding: .utf8)
        }
        let connected = await expect("the host appears once the install command has run", within: 90) {
            store.hosts.first?.state == .connected
        }
        guard connected else { return finish() }
        await shoot("03-add-project")

        store.showsFolderPicker = true
        await wait(1)
        await shoot("03-folder-picker")
        store.showsFolderPicker = false
        await wait(0.6)

        guard let host = store.hosts.first else { return finish() }
        store.addProject(hostID: host.id, path: environment["MOTILE_DEMO_PROJECT"] ?? NSTemporaryDirectory())
        await expect("a folder on the host becomes a project") { store.project(store.newThread.projectID) != nil }
        await expect("the project's icon is fetched from the host") { store.project(store.newThread.projectID)?.iconPath != nil }
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

        // A thread that ends asking for permission.
        store.startNewThread()
        store.setAccess(.supervised)
        send("Change greet.py to use an f-string, run it, and summarize the change in a table.")
        await expect("a supervised turn ends asking for approval", within: 60) { turnEnded && store.selectedThread?.needsApproval == true }
        await shoot("07-approval")

        // A long reply full of code, to see that the window keeps up.
        store.startNewThread()
        store.setAccess(.full)
        send("Give me a long reply with a lot of code.")
        await wait(0.5)
        let longStreaming = StallMonitor()
        await expect("a long reply arrives whole", within: 90) { turnEnded }
        results.append(responsive("a long reply streams", longStreaming))
        let scrolling = StallMonitor()
        await scrollTranscript()
        results.append(responsive("a long reply is scrolled", scrolling))
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

        NSApp.appearance = NSAppearance(named: .darkAqua)
        await shoot("11-dark-thread")
        await shootScreen("11-screen-dark")
        store.setDone([first], done: false)
        await expect("a thread marked undone is active again") { store.doneThreads.isEmpty }
        store.startNewThread()
        store.draft = "Why is the sync slow on large threads?\n\nLook at how the host answers `Open` first:\n- what it reads from SQLite\n- how many items it sends"
        store.attach([URL(fileURLWithPath: "/tmp/motile-demo/api/greet.py")])
        await shoot("12-dark-new-thread")
        store.attachments = []
        store.draft = ""
        finish()
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
        case .group: "group"
        case .fold: "fold"
        case .error: "error"
        case .turnEnd: "turn_end"
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
