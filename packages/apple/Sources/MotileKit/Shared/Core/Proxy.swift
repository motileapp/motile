import Foundation
import Observation

/// The account's proxy as the Proxy page shows it, from the core's `proxy` event.
struct ProxyState: Equatable {
    enum Connection: String {
        case off, connecting, connected, disconnected, refused
    }

    var exists = false
    var name: String?
    var key: String?
    var connection = Connection.off
    /// "Connected", "Can't reach your proxy".
    var statusText = "Not connected"
    var error: String?
    var version: String?
    /// Claude Code's first, each agent's by priority.
    var accounts: [ProxyAccount] = []
    var capture = ProxyCapture()
    var signIn: ProxySignIn?
    /// The newest entry of the request log, which changes when a request comes.
    var logHead: Int?

    func accounts(of agent: Agent) -> [ProxyAccount] {
        accounts.filter { $0.agent == agent }
    }
}

extension ProxyState {
    init(json: JSON) {
        exists = json.bool("exists")
        name = json.optionalString("name")
        key = json.optionalString("key")
        connection = Connection(rawValue: json.string("connection")) ?? .off
        statusText = json.string("status_text")
        error = json.optionalString("error")
        version = json.optionalString("version")
        accounts = json.objects("accounts").map(ProxyAccount.init)
        capture = ProxyCapture(json: json.object("capture") ?? [:])
        signIn = json.object("sign_in").map(ProxySignIn.init)
        logHead = (json["log_head"] as? NSNumber)?.intValue
    }
}

/// An account of the proxy's pool: a Claude organisation or a Codex account.
struct ProxyAccount: Equatable, Identifiable {
    enum Status: String {
        case ready, off, resting
        case signInAgain = "sign_in_again"
    }

    let id: String
    let agent: Agent
    let name: String
    /// Who is signed in.
    let email: String?
    let plan: String?
    let status: Status
    /// "Off", "Sign in again", "Resting until 14:30". None while it is ready.
    let statusText: String?
    /// Lower is used first.
    let priority: Int
    /// "Priority 1".
    let priorityText: String
    let enabled: Bool
    /// New threads of its agent start on it.
    let next: Bool
    let windows: [LimitsReport.Window]
    /// Why there are no windows.
    let note: String?
    /// "1 thread": the threads on it now. None when none is.
    let threadsText: String?

    init(json: JSON) {
        id = json.string("id")
        agent = Agent(rawValue: json.string("agent")) ?? .claude
        name = json.string("name")
        email = json.optionalString("email")
        plan = json.optionalString("plan")
        status = Status(rawValue: json.string("status")) ?? .ready
        statusText = json.optionalString("status_text")
        priority = json.int("priority")
        priorityText = json.string("priority_text")
        enabled = json.bool("enabled")
        next = json.bool("next")
        windows = json.objects("windows").map(LimitsReport.Window.init(json:))
        note = json.optionalString("note")
        threadsText = json.optionalString("threads_text")
    }

    /// "a@b.c · Max 20x · 1 thread", or whichever of them is known.
    var description: String? {
        let parts = [email, plan, threadsText].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// What the proxy keeps in full of the requests it passes on.
struct ProxyCapture: Equatable {
    enum Kind: Equatable {
        case off
        case next(Int)
        case thread(server: String, thread: String)
    }

    var kind = Kind.off
    var on = false
    /// "Not capturing.", "Capturing the next 20 requests, until 14:30."
    var text = "Not capturing."

    init() {}

    init(json: JSON) {
        let capture = json.object("capture") ?? [:]
        switch capture.string("type") {
        case "next": kind = .next(capture.int("left"))
        case "thread": kind = .thread(server: capture.string("server"), thread: capture.string("thread"))
        default: kind = .off
        }
        on = json.bool("on")
        text = json.string("text")
    }

    static func command(_ kind: Kind) -> JSON {
        switch kind {
        case .off: ["type": "off"]
        case .next(let left): ["type": "next", "left": left]
        case .thread(let server, let thread): ["type": "thread", "server": server, "thread": thread]
        }
    }
}

/// An account's sign-in under way on the proxy.
struct ProxySignIn: Equatable, Identifiable {
    let agent: Agent
    let url: URL?
    /// The code to enter on the page (Codex).
    let userCode: String?
    /// The page shows a code to paste back (Claude).
    let needsCode: Bool
    /// The proxy is finishing it.
    let waiting: Bool
    let text: String
    let error: String?

    init(json: JSON) {
        agent = Agent(rawValue: json.string("agent")) ?? .claude
        url = URL(string: json.string("url"))
        userCode = json.optionalString("user_code")
        needsCode = json.bool("needs_code")
        waiting = json.bool("waiting")
        text = json.string("text")
        error = json.optionalString("error")
    }

    var id: String { agent.rawValue }
}

/// How to run a new proxy: the code it joins the account with, its variables and the command
/// that does it all.
struct ProxyEnroll {
    struct Variable: Identifiable {
        let name: String
        let value: String

        var id: String { name }
    }

    let code: String
    let expiresAt: Double
    let image: String
    let variables: [Variable]
    let command: String
    let spans: [Int]
    let steps: [String]

    init(json: JSON) {
        code = json.string("code")
        expiresAt = json.double("expires_at")
        image = json.string("image")
        variables = json.objects("variables").map { Variable(name: $0.string("name"), value: $0.string("value")) }
        command = json.string("command")
        spans = (json["spans"] as? [NSNumber] ?? []).map(\.intValue)
        steps = json.strings("steps")
    }

    func isExpired(at date: Date) -> Bool {
        expiresAt <= date.timeIntervalSince1970
    }

    /// How long the code still works, as "14:59".
    func timeLeft(at date: Date) -> String {
        let seconds = max(0, Int((expiresAt - date.timeIntervalSince1970).rounded(.up)))
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}

/// A request in the proxy's log.
struct ProxyLogRow: Equatable, Identifiable {
    let id: Int
    /// "14:32:05" today, "Nov 1, 14:32" before.
    let time: String
    let server: String
    let thread: String
    let account: String?
    let model: String?
    let path: String
    let status: Int
    let ok: Bool
    /// "340 ms".
    let latency: String
    let tokens: String
    let retries: String?
    let size: String
    let error: String?
    let captured: Bool

    init(json: JSON) {
        id = json.int("id")
        time = json.string("time")
        server = json.string("server")
        thread = json.string("thread")
        account = json.optionalString("account")
        model = json.optionalString("model")
        path = json.string("path")
        status = json.int("status")
        ok = json.bool("ok")
        latency = json.string("latency")
        tokens = json.string("tokens")
        retries = json.optionalString("retries")
        size = json.string("size")
        error = json.optionalString("error")
        captured = json.bool("captured")
    }

    /// "Work · claude-opus-5-5 · /v1/messages · 12k in · 980 out · 1 retry · 1.2 MB sent · 40 KB received".
    var details: String {
        ([account, model, path, tokens, retries, size] as [String?]).compactMap { $0 }.joined(separator: " · ")
    }

    /// "14:32:05 · studio · Fix the build · 200 /v1/messages · 340 ms".
    var summary: String {
        [time, server, thread, "\(status) \(path)", latency].joined(separator: " · ")
    }
}

/// A page of the log, newest first.
struct ProxyLogPage {
    let rows: [ProxyLogRow]
    let more: Bool

    init(json: JSON) {
        rows = json.objects("rows").map(ProxyLogRow.init)
        more = json.bool("more")
    }
}

/// A captured request in full: the request and the response, each its headers then its body,
/// laid out off the main thread.
struct ProxyCaptured {
    let row: ProxyLogRow
    let request: CodeDocument
    let response: CodeDocument

    /// Nil when the proxy no longer has it.
    init?(json: JSON) {
        guard let captured = json.object("request") else { return nil }
        let row = ProxyLogRow(json: captured.object("row") ?? [:])
        let part = { (name: String, headers: String, body: String) in
            let headers = (captured[headers] as? [[String]] ?? []).map { $0.joined(separator: ": ") }
            let files = [CodeFile(path: "Headers", lines: headers), CodeFile(path: "Body", lines: captured.string(body).components(separatedBy: "\n"))]
            return CodeDocument(id: "proxy-\(name)-\(row.id)", files: files, truncated: false, headed: true)
        }
        self.row = row
        request = part("request", "request_headers", "request_body")
        response = part("response", "response_headers", "response_body")
    }
}

/// A captured request the page shows, by its entry in the log.
struct ProxyCapturedEntry: Identifiable {
    let id: Int
}

/// The proxy's request log as the page pages through it: all of it or only what failed.
@Observable
final class ProxyLog {
    private(set) var rows: [ProxyLogRow] = []
    private(set) var more = false
    private(set) var loading = false
    private(set) var error: String?
    var errorsOnly = false {
        didSet {
            guard errorsOnly != oldValue else { return }
            rows = []
            more = false
            reload()
        }
    }
    @ObservationIgnored private weak var store: AppStore?
    /// Counts what was asked, so that only the answer to the last is kept.
    @ObservationIgnored private var asked = 0

    func start(_ store: AppStore) {
        self.store = store
        reload()
    }

    /// Reads the newest page again, keeping the rows shown until it comes.
    func reload() {
        load(before: nil)
    }

    func loadMore() {
        guard more, !loading, let last = rows.last else { return }
        load(before: last.id)
    }

    /// Puts the requests that came since the newest row shown on top, or starts over from the
    /// newest page when more came than a page holds.
    func refresh() {
        guard let store, !loading else { return }
        let number = asked
        store.loadProxyLog(before: nil, errorsOnly: errorsOnly) { [weak self] result in
            guard let self, number == asked, case .success(let page) = result else { return }
            let newest = rows.first?.id ?? 0
            let fresh = page.rows.filter { $0.id > newest }
            guard fresh.count < page.rows.count || rows.isEmpty else {
                rows = page.rows
                more = page.more
                return
            }
            rows = fresh + rows
        }
    }

    private func load(before: Int?) {
        guard let store else { return }
        asked += 1
        let number = asked
        loading = true
        error = nil
        store.loadProxyLog(before: before, errorsOnly: errorsOnly) { [weak self] result in
            guard let self, number == asked else { return }
            loading = false
            switch result {
            case .success(let page):
                rows = before == nil ? page.rows : rows + page.rows
                more = page.more
            case .failure(let failure):
                error = failure.message
            }
        }
    }
}

extension AppStore {
    /// While a view shows the proxy, the core stays connected to it and follows its state.
    func watchProxy(_ on: Bool) {
        let before = proxyWatchers
        proxyWatchers = max(0, proxyWatchers + (on ? 1 : -1))
        guard (before == 0) != (proxyWatchers == 0) else { return }
        core.send("watch_proxy", ["on": proxyWatchers > 0])
    }

    /// Makes the code a new proxy joins the account with.
    func addProxy(reply: @escaping (Result<ProxyEnroll, CoreBridge.CoreError>) -> Void) {
        core.send("add_proxy") { result in reply(result.map { ProxyEnroll(json: $0) }) }
    }

    /// While the account is watched, a proxy that joins it shows up at once.
    func watchAccount(_ on: Bool) {
        core.send("watch_servers", ["on": on])
    }

    func removeProxy() {
        proxyCommand("remove_proxy")
    }

    /// Starts signing an account in on the proxy, and opens the page it signs in on.
    func startProxySignIn(_ agent: Agent) {
        guard proxySignInStarting == nil else { return }
        proxySignInStarting = agent
        core.send("proxy_sign_in", ["agent": agent.rawValue]) { [weak self] result in
            self?.proxySignInStarting = nil
            switch result {
            case .success(let value):
                guard let url = URL(string: value.string("url")) else { return }
                Platform.open(url)
            case .failure(let error):
                self?.alert(error)
            }
        }
    }

    /// Finishes Claude's sign-in with the code its page showed. Its error shows in the sign-in.
    func finishProxySignIn(code: String) {
        core.send("proxy_finish_sign_in", ["code": code.trimmingCharacters(in: .whitespacesAndNewlines)])
    }

    func cancelProxySignIn() {
        core.send("proxy_cancel_sign_in")
    }

    func renameProxyAccount(_ account: ProxyAccount, to name: String) {
        updateProxyAccount(account, ["type": "name", "name": name.trimmingCharacters(in: .whitespaces)])
    }

    /// `priority` counts from 0, which is used first.
    func setProxyAccountPriority(_ account: ProxyAccount, to priority: Int) {
        updateProxyAccount(account, ["type": "priority", "priority": priority])
    }

    func setProxyAccount(_ account: ProxyAccount, enabled: Bool) {
        updateProxyAccount(account, ["type": "enabled", "enabled": enabled])
    }

    func removeProxyAccount(_ account: ProxyAccount) {
        proxyCommand("proxy_remove_account", ["account": account.id])
    }

    func setProxyCapture(_ kind: ProxyCapture.Kind) {
        proxyCommand("proxy_capture", ["capture": ProxyCapture.command(kind)])
    }

    func loadProxyLog(before: Int?, errorsOnly: Bool, reply: @escaping (Result<ProxyLogPage, CoreBridge.CoreError>) -> Void) {
        var command: JSON = ["errors_only": errorsOnly]
        if let before { command["before"] = before }
        core.send("proxy_log", command, read: ProxyLogPage.init, reply: reply)
    }

    /// A captured request in full, or nil when the proxy no longer has it.
    func loadProxyCaptured(_ entry: Int, reply: @escaping (Result<ProxyCaptured?, CoreBridge.CoreError>) -> Void) {
        core.send("proxy_captured", ["entry": entry], read: ProxyCaptured.init, reply: reply)
    }

    private func updateProxyAccount(_ account: ProxyAccount, _ change: JSON) {
        proxyCommand("proxy_update_account", ["account": account.id, "change": change])
    }

    private func proxyCommand(_ type: String, _ fields: JSON = [:]) {
        core.send(type, fields) { [weak self] result in
            if case .failure(let error) = result { self?.alert(error) }
        }
    }
}
