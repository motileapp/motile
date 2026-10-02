import AppKit
import AuthenticationServices
import Foundation
import Observation
import UniformTypeIdentifiers

enum Selection: Hashable {
    case newThread
    case thread(String)
}

/// What a new thread starts with: the composer's choices before there is a thread to hold them.
struct NewThreadSettings: Equatable {
    var projectID: String?
    var model: String?
    var effort: String?
    var access: Access = .full
    var plan = false
}

/// Where the command panel opens.
enum PanelPage: Equatable {
    /// Everything that can be done from here.
    case commands
    /// The projects, to start a thread in one.
    case projects
    /// The threads, to open one.
    case threads
}

struct UndoNotice: Equatable {
    let threadIDs: [String]
    let text: String
}

/// Everything the views show. It mirrors the core: commands go to it, and its events change the
/// state here. All of it is used on the main thread.
@Observable
final class AppStore {
    // Account
    private(set) var account = Account()
    /// Whether the core has said who is signed in. Until then nothing is shown.
    private(set) var ready = false
    private(set) var signingIn = false
    var signInError: String?
    private(set) var enrollToken: EnrollToken?
    var showsAddHost = false
    var showsFolderPicker = false
    private(set) var panel: PanelPage?
    /// Counts up when the composer should take the keyboard back.
    private(set) var composerFocus = 0

    // What the hosts hold
    private(set) var hosts: [Host] = []
    private(set) var projects: [Project] = []
    private(set) var threads: [String: ThreadInfo] = [:]

    // The open thread
    private(set) var selection: Selection = .newThread
    private(set) var activity = Activity()
    private(set) var transcriptIsEmpty = true
    private(set) var sending = false
    var errorMessage: String?
    var newThread = NewThreadSettings()
    private(set) var undo: UndoNotice?
    var drafts: [String: String] = [:]
    var attachments: [String] = []

    @ObservationIgnored let core = CoreBridge()
    @ObservationIgnored let transcript = TranscriptModel()
    @ObservationIgnored private var signInSession: SignInSession?
    @ObservationIgnored private var undoTimer: Timer?
    @ObservationIgnored private var openThreadID: String?
    @ObservationIgnored private let defaults = UserDefaults.standard
    /// The thread that was open when the app was last closed, until it has been opened again.
    @ObservationIgnored private var lastSelection: String?

    init() {
        loadPreferences()
    }

    // MARK: Starting

    func start() {
        core.decode = { [weak self] event in self?.decode(event) }
        let environment = ProcessInfo.processInfo.environment
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
        let dataDir = environment["MOTILE_DATA_DIR"] ?? support?.appendingPathComponent("Motile").path ?? NSTemporaryDirectory()
        let bundled = Bundle.main.object(forInfoDictionaryKey: "MotileAuthURL") as? String
        let authURL = environment["MOTILE_AUTH_URL"] ?? bundled.flatMap { $0.isEmpty ? nil : $0 } ?? "https://auth.motile.app"
        var config: JSON = [
            "data_dir": dataDir,
            "auth_url": authURL,
            "device_name": deviceName(),
            "platform": "macos",
            "local_only": environment["MOTILE_LOCAL"] == "1",
        ]
        if let address = environment["MOTILE_HOST_ADDR"] { config["direct_addr"] = address }
        if !core.start(config: config) {
            errorMessage = "Motile couldn't start. Its data folder may not be writable."
        }
    }

    // MARK: Events

    /// Runs off the main thread: reads the event and returns what to do with it on the main thread.
    private func decode(_ event: JSON) -> (() -> Void)? {
        switch event.string("type") {
        case "account":
            let account = Account(json: event.object("account") ?? [:])
            return { [weak self] in self?.apply(account) }
        case "hosts":
            let hosts = event.objects("hosts").map { Host(json: $0) }
            return { [weak self] in self?.apply(hosts: hosts) }
        case "threads":
            let hostID = event.string("host_id")
            let threads = event.objects("threads").map { ThreadInfo(json: $0) }
            return { [weak self] in self?.apply(threads: threads, hostID: hostID) }
        case "thread_upsert":
            let thread = ThreadInfo(json: event.object("thread") ?? [:])
            return { [weak self] in self?.upsert(thread) }
        case "thread_deleted":
            let threadID = event.string("thread_id")
            return { [weak self] in self?.removeThread(threadID) }
        case "projects":
            let hostID = event.string("host_id")
            let projects = event.objects("projects").map { Project(json: $0, hostID: hostID) }
            return { [weak self] in self?.apply(projects: projects, hostID: hostID) }
        case "rows":
            let threadID = event.string("thread_id")
            let (reset, start, remove) = (event.bool("reset"), event.int("start"), event.int("remove"))
            let rows = event.objects("rows").compactMap { RowModel(json: $0) }
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID else { return }
                self.transcript.apply(reset: reset, start: start, remove: remove, rows: rows)
                // Assigned only when it changes: every assignment makes the views that read it
                // update, and rows arrive many times a second.
                let isEmpty = self.transcript.isEmpty
                if self.transcriptIsEmpty != isEmpty { self.transcriptIsEmpty = isEmpty }
            }
        case "spans":
            let (threadID, rowID) = (event.string("thread_id"), event.string("row_id"))
            let spans = event["spans"] as? [NSNumber] ?? []
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID else { return }
                self.transcript.apply(spans: spans, rowID: rowID)
            }
        case "activity":
            let threadID = event.string("thread_id")
            let activity = Activity(json: event.object("activity") ?? [:])
            return { [weak self] in
                guard let self, self.transcript.threadID == threadID else { return }
                if self.activity != activity { self.activity = activity }
                self.transcript.setActivity(activity)
            }
        case "thread_error":
            let message = event.string("message")
            return { [weak self] in self?.errorMessage = message }
        default:
            return nil
        }
    }

    private func apply(_ account: Account) {
        let wasSignedIn = self.account.signedIn
        self.account = account
        ready = true
        if wasSignedIn && !account.signedIn {
            select(.newThread)
            enrollToken = nil
        }
    }

    private func apply(hosts: [Host]) {
        self.hosts = hosts
        let known = Set(hosts.map(\.id))
        projects.removeAll { !known.contains($0.hostID) }
        threads = threads.filter { known.contains($0.value.hostID) }
        ensureNewThreadDefaults()
        // The host has arrived; the install command has done its job.
        if showsAddHost, hosts.count > addHostCount {
            showsAddHost = false
        }
    }

    private func apply(threads new: [ThreadInfo], hostID: String) {
        threads = threads.filter { $0.value.hostID != hostID }
        for thread in new { threads[thread.id] = thread }
        if case .thread(let id) = selection, threads[id] == nil {
            select(.newThread)
        }
        restoreSelection()
    }

    private func upsert(_ thread: ThreadInfo) {
        threads[thread.id] = thread
        // A reply that arrives while the thread is open has been seen.
        if thread.unread, selection == .thread(thread.id), NSApp.isActive {
            core.send("mark_seen", ["thread_id": thread.id])
        }
    }

    private func removeThread(_ id: String) {
        threads[id] = nil
        if selection == .thread(id) { select(.newThread) }
    }

    private func apply(projects new: [Project], hostID: String) {
        projects.removeAll { $0.hostID == hostID }
        projects.append(contentsOf: new)
        projects.sort { $0.createdAt < $1.createdAt }
        ImageFiles.shared.warm(new.compactMap(\.iconPath))
        ensureNewThreadDefaults()
    }

    // MARK: Lookups

    var activeThreads: [ThreadInfo] {
        threads.values.filter { !$0.isDone }.sorted { ($0.activeOrder, $0.id) > ($1.activeOrder, $1.id) }
    }

    var doneThreads: [ThreadInfo] {
        threads.values.filter(\.isDone).sorted { ($0.doneAt ?? 0, $0.id) > ($1.doneAt ?? 0, $1.id) }
    }

    var selectedThread: ThreadInfo? {
        guard case .thread(let id) = selection else { return nil }
        return threads[id]
    }

    func project(_ id: String?) -> Project? {
        projects.first { $0.id == id }
    }

    func host(_ id: String?) -> Host? {
        hosts.first { $0.id == id }
    }

    /// The host the composer is talking to: the open thread's, or the new thread's project's.
    var composerHost: Host? {
        if let thread = selectedThread { return host(thread.hostID) }
        return host(project(newThread.projectID)?.hostID) ?? hosts.first
    }

    /// The models the composer offers: an open thread stays with its agent.
    var composerModels: [ModelInfo] {
        let models = composerHost?.models ?? []
        guard let thread = selectedThread else { return models }
        return models.filter { $0.agent == thread.agent }
    }

    var composerModel: ModelInfo? {
        let id = selectedThread.map { $0.model } ?? newThread.model
        return composerModels.first { $0.id == id } ?? composerModels.first
    }

    var composerEffort: String? {
        let effort = selectedThread.map { $0.effort } ?? newThread.effort
        guard let model = composerModel, !model.efforts.isEmpty else { return nil }
        if let effort, model.efforts.contains(effort) { return effort }
        return model.defaultEffort ?? model.efforts.first
    }

    var composerAccess: Access { selectedThread?.access ?? newThread.access }
    var composerPlan: Bool { selectedThread?.plan ?? newThread.plan }

    var draftKey: String {
        if case .thread(let id) = selection { return id }
        return "new"
    }

    var draft: String {
        get { drafts[draftKey] ?? "" }
        set {
            drafts[draftKey] = newValue.isEmpty ? nil : newValue
            defaults.set(drafts, forKey: "drafts")
        }
    }

    // MARK: Preferences

    private func loadPreferences() {
        drafts = defaults.dictionary(forKey: "drafts") as? [String: String] ?? [:]
        newThread.projectID = defaults.string(forKey: "new.project")
        newThread.model = defaults.string(forKey: "new.model")
        newThread.effort = defaults.string(forKey: "new.effort")
        newThread.access = Access(rawValue: defaults.string(forKey: "new.access") ?? "") ?? .full
        lastSelection = defaults.string(forKey: "selection")
    }

    private func savePreferences() {
        defaults.set(newThread.projectID, forKey: "new.project")
        defaults.set(newThread.model, forKey: "new.model")
        defaults.set(newThread.effort, forKey: "new.effort")
        defaults.set(newThread.access.rawValue, forKey: "new.access")
    }

    /// Keeps the new thread's project and model pointing at things that exist.
    private func ensureNewThreadDefaults() {
        if project(newThread.projectID) == nil {
            newThread.projectID = projects.first?.id
        }
    }

    /// Opens the thread that was open when the app was last closed, once it is known.
    private func restoreSelection() {
        guard let wanted = lastSelection, threads[wanted] != nil else { return }
        lastSelection = nil
        guard selection == .newThread else { return }
        select(.thread(wanted))
    }

    // MARK: Account

    func signIn() {
        guard !signingIn else { return }
        signingIn = true
        signInError = nil
        core.send("begin_sign_in") { [weak self] result in
            guard let self else { return }
            guard case .success(let value) = result, let url = URL(string: value.string("url")) else {
                self.signInFailed("The sign-in couldn't be started.")
                return
            }
            let session = SignInSession()
            self.signInSession = session
            session.start(url: url) { [weak self] callback in
                guard let self else { return }
                self.signInSession = nil
                guard let callback else {
                    self.signingIn = false
                    return
                }
                self.core.send("complete_sign_in", ["url": callback.absoluteString]) { [weak self] result in
                    if case .failure(let error) = result { self?.signInFailed(error.message) } else { self?.signingIn = false }
                }
            }
        }
    }

    /// Signs in on an auth server that allows it without Google. Used by the demo and local work.
    func devSignIn(email: String, done: (() -> Void)? = nil) {
        signingIn = true
        core.send("dev_sign_in", ["email": email]) { [weak self] result in
            if case .failure(let error) = result { self?.signInFailed(error.message) } else { self?.signingIn = false }
            done?()
        }
    }

    private func signInFailed(_ message: String) {
        signingIn = false
        signInError = message
    }

    func signOut() {
        defaults.removeObject(forKey: "drafts")
        defaults.removeObject(forKey: "selection")
        drafts = [:]
        core.send("sign_out")
    }

    // MARK: Hosts

    @ObservationIgnored private var addHostCount = 0

    /// Asks for an install command and keeps looking for the host it will link.
    func prepareToAddHost() {
        addHostCount = hosts.count
        core.send("watch_hosts", ["on": true])
        if let token = enrollToken, token.expiresAt - Date().timeIntervalSince1970 > 600 { return }
        core.send("create_enroll_token") { [weak self] result in
            switch result {
            case .success(let value): self?.enrollToken = EnrollToken(json: value)
            case .failure(let error): self?.errorMessage = error.message
            }
        }
    }

    func stopAddingHost() {
        core.send("watch_hosts", ["on": false])
        // A token links one host; the next host gets a new one.
        if hosts.count != addHostCount { enrollToken = nil }
    }

    func removeHost(_ host: Host) {
        core.send("remove_host", ["host_id": host.id]) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    // MARK: Projects

    func listFolder(hostID: String, path: String?, done: @escaping (Result<RemoteFolder, CoreBridge.CoreError>) -> Void) {
        var request: JSON = ["type": "list_dir"]
        if let path { request["path"] = path }
        core.send("request", ["host_id": hostID, "request": request]) { result in
            done(result.map { RemoteFolder(json: $0) })
        }
    }

    func addProject(hostID: String, path: String) {
        request(hostID, ["type": "add_project", "path": path]) { [weak self] in
            guard let self else { return }
            // The new project is the one the next thread starts in.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                let trimmed = path.count > 1 && path.hasSuffix("/") ? String(path.dropLast()) : path
                guard let project = self.projects.first(where: { $0.hostID == hostID && $0.path == trimmed }) else { return }
                self.setNewThreadProject(project.id)
            }
        }
    }

    func removeProject(_ project: Project) {
        request(project.hostID, ["type": "remove_project", "project_id": project.id])
    }

    /// Makes an image on this Mac the project's icon. Without one, the project goes back to the
    /// icon found in its folder.
    func setIcon(of project: Project, to file: URL?) {
        var command: JSON = ["host_id": project.hostID, "project_id": project.id]
        if let file { command["file"] = file.path }
        core.send("set_project_icon", command) { [weak self] result in
            if case .failure(let error) = result { self?.errorMessage = error.message }
        }
    }

    func chooseIcon(for project: Project) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        panel.message = "Choose an icon for \(project.name)"
        guard panel.runModal() == .OK, let file = panel.url else { return }
        setIcon(of: project, to: file)
    }

    // MARK: Command panel

    func openPanel(_ page: PanelPage) {
        guard account.signedIn, !hosts.isEmpty else { return }
        panel = page
    }

    func closePanel() {
        guard panel != nil else { return }
        panel = nil
        composerFocus += 1
    }

    func startNewThread(in project: Project) {
        setNewThreadProject(project.id)
        select(.newThread)
    }

    // MARK: Threads

    func select(_ new: Selection) {
        lastSelection = nil
        guard new != selection || openThreadID == nil else { return }
        if let open = openThreadID {
            core.send("close_thread", ["thread_id": open])
            openThreadID = nil
        }
        selection = new
        activity = Activity()
        transcriptIsEmpty = true
        attachments = []
        guard case .thread(let id) = new, let thread = threads[id] else {
            transcript.begin(threadID: nil)
            defaults.removeObject(forKey: "selection")
            return
        }
        open(thread)
    }

    private func open(_ thread: ThreadInfo) {
        openThreadID = thread.id
        transcript.begin(threadID: thread.id)
        defaults.set(thread.id, forKey: "selection")
        core.send("open_thread", ["host_id": thread.hostID, "thread_id": thread.id])
        core.send("mark_seen", ["thread_id": thread.id])
    }

    func startNewThread() {
        select(.newThread)
    }

    func setNewThreadProject(_ id: String?) {
        newThread.projectID = id
        savePreferences()
    }

    var canSend: Bool {
        guard !sending, let host = composerHost, host.state == .connected else { return false }
        if selectedThread == nil && project(newThread.projectID) == nil { return false }
        return !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !attachments.isEmpty
    }

    func send() {
        guard canSend else { return }
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        let files = attachments
        var command: JSON = ["text": text, "files": files]
        let existing = selectedThread
        if let thread = existing {
            command["host_id"] = thread.hostID
            command["thread_id"] = thread.id
        } else {
            guard let project = project(newThread.projectID), let model = composerModel else {
                errorMessage = "This host has no agent installed. Install Claude Code or Codex on it and try again."
                return
            }
            var settings: JSON = [
                "project_id": project.id,
                "agent": model.agent.rawValue,
                "model": model.id,
                "access": newThread.access.rawValue,
                "plan": newThread.plan,
            ]
            if let effort = composerEffort { settings["effort"] = effort }
            command["host_id"] = project.hostID
            command["new_thread"] = settings
            sending = true
            activity = Activity.starting
            transcript.setActivity(activity)
        }
        draft = ""
        attachments = []
        transcript.setPending(text)
        transcriptIsEmpty = false

        core.send("send", command) { [weak self] result in
            guard let self else { return }
            self.sending = false
            switch result {
            case .failure(let error):
                self.transcript.setPending(nil)
                self.transcriptIsEmpty = self.transcript.isEmpty
                self.activity = existing == nil ? Activity() : self.activity
                self.transcript.setActivity(self.activity)
                self.draft = text
                self.attachments = files
                self.errorMessage = error.message
            case .success(let value):
                guard existing == nil else { return }
                self.openNewThread(id: value.string("thread_id"))
            }
        }
    }

    /// Switches to the thread a first message created, keeping the message on screen meanwhile.
    private func openNewThread(id: String) {
        guard selection == .newThread, let hostID = project(newThread.projectID)?.hostID else { return }
        selection = .thread(id)
        openThreadID = id
        defaults.set(id, forKey: "selection")
        transcript.adopt(threadID: id)
        core.send("open_thread", ["host_id": hostID, "thread_id": id])
        core.send("mark_seen", ["thread_id": id])
    }

    func stop() {
        guard let thread = selectedThread else { return }
        request(thread.hostID, ["type": "stop", "thread_id": thread.id])
    }

    func allow(_ denials: [Denial]) {
        guard let thread = selectedThread else { return }
        request(thread.hostID, ["type": "allow", "thread_id": thread.id, "denials": denials.map(\.json)])
    }

    func rename(_ thread: ThreadInfo, to title: String) {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty, title != thread.title else { return }
        update(thread, ["title": title]) { $0.title = title }
    }

    func delete(_ thread: ThreadInfo) {
        request(thread.hostID, ["type": "delete", "thread_id": thread.id])
    }

    func setModel(_ model: ModelInfo) {
        guard let thread = selectedThread else {
            newThread.model = model.id
            newThread.effort = nil
            savePreferences()
            return
        }
        update(thread, ["model": model.id, "effort": ""]) {
            $0.model = model.id
            $0.effort = nil
        }
    }

    func setEffort(_ effort: String) {
        guard let thread = selectedThread else {
            newThread.effort = effort
            savePreferences()
            return
        }
        update(thread, ["effort": effort]) { $0.effort = effort }
    }

    func setAccess(_ access: Access) {
        guard let thread = selectedThread else {
            newThread.access = access
            savePreferences()
            return
        }
        update(thread, ["access": access.rawValue]) { $0.access = access }
    }

    func setPlan(_ plan: Bool) {
        guard let thread = selectedThread else {
            newThread.plan = plan
            return
        }
        update(thread, ["plan": plan]) { $0.plan = plan }
    }

    /// Marks threads done or brings them back. A thread that is working can't be marked done.
    func setDone(_ ids: [String], done: Bool, fromSidebar: Bool = false) {
        let changed = ids.compactMap { threads[$0] }.filter { $0.isDone != done && !(done && $0.running) }
        guard !changed.isEmpty else { return }
        // Leaving the thread that was just put away, for the next one that is still active.
        if done, fromSidebar, case .thread(let open) = selection, changed.contains(where: { $0.id == open }) {
            let active = activeThreads
            let position = active.firstIndex { $0.id == open } ?? 0
            let remaining = active.filter { thread in !changed.contains { $0.id == thread.id } }
            let next = remaining.isEmpty ? nil : remaining[min(position, remaining.count - 1)]
            select(next.map { .thread($0.id) } ?? .newThread)
        }
        let now = Date().timeIntervalSince1970
        for thread in changed {
            update(thread, ["done": done]) { $0.doneAt = done ? now : nil }
        }
        guard done else {
            undo = nil
            return
        }
        showUndo(UndoNotice(threadIDs: changed.map(\.id), text: changed.count == 1 ? "Marked done" : "Marked \(changed.count) threads done"))
    }

    func toggleDone() {
        guard let thread = selectedThread else { return }
        setDone([thread.id], done: !thread.isDone)
    }

    private func showUndo(_ notice: UndoNotice) {
        undo = notice
        undoTimer?.invalidate()
        undoTimer = Timer.scheduledTimer(withTimeInterval: 5, repeats: false) { [weak self] _ in self?.undo = nil }
    }

    func performUndo() {
        guard let notice = undo else { return }
        undo = nil
        setDone(notice.threadIDs, done: false)
    }

    /// Changes a thread on its host, and here at once so the app doesn't wait for the answer.
    private func update(_ thread: ThreadInfo, _ change: JSON, locally: (inout ThreadInfo) -> Void) {
        var changed = thread
        locally(&changed)
        threads[thread.id] = changed
        request(thread.hostID, ["type": "update", "thread_id": thread.id, "change": change], failed: { [weak self] in
            self?.threads[thread.id] = thread
        })
    }

    private func request(_ hostID: String, _ request: JSON, done: (() -> Void)? = nil, failed: (() -> Void)? = nil) {
        core.send("request", ["host_id": hostID, "request": request]) { [weak self] result in
            switch result {
            case .success: done?()
            case .failure(let error):
                failed?()
                self?.errorMessage = error.message
            }
        }
    }

    // MARK: Attachments

    func attach(_ urls: [URL]) {
        for url in urls where url.isFileURL && !attachments.contains(url.path) {
            attachments.append(url.path)
        }
    }
}

extension Activity {
    /// What is shown between sending a first message and the host saying the turn runs.
    static var starting: Activity {
        var activity = Activity()
        activity.running = true
        activity.startedAt = Date().timeIntervalSince1970
        return activity
    }
}

private func deviceName() -> String {
    Foundation.Host.current().localizedName ?? "Mac"
}

/// Shows the browser sheet for signing in and reports the address it was sent back to.
final class SignInSession: NSObject, ASWebAuthenticationPresentationContextProviding {
    private var session: ASWebAuthenticationSession?

    func start(url: URL, done: @escaping (URL?) -> Void) {
        let session = ASWebAuthenticationSession(url: url, callbackURLScheme: "motile") { callback, _ in
            DispatchQueue.main.async { done(callback) }
        }
        session.presentationContextProvider = self
        self.session = session
        if !session.start() {
            done(nil)
        }
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        NSApp.keyWindow ?? NSApp.windows.first ?? ASPresentationAnchor()
    }
}
