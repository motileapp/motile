import Foundation
import Observation

/// The account's gateway as the Gateway page shows it, from the core's `gateway` event.
struct GatewayState: Equatable {
    enum Connection: String {
        case off, connecting, connected, disconnected, refused
    }

    var exists = false
    var name: String?
    var key: String?
    var connection = Connection.off
    /// "Connected", "Can't reach your gateway".
    var statusText = "Not connected"
    var error: String?
    var version: String?
    /// Claude Code's first, each agent's by priority.
    var accounts: [GatewayAccount] = []
    var capture = GatewayCapture()
    var signIn: GatewaySignIn?
    /// The newest entry of the request log, which changes when a request comes.
    var logHead: Int?
    /// Nil while the gateway isn't known, or is too old to have MCP servers.
    var mcpServers: [McpServerRow]?
    /// The newest call of the MCP servers, which changes when an agent calls one.
    var callsHead: Int?
    /// The MCPs the add sheet offers.
    var known: [KnownMcpServer] = []

    func accounts(of agent: Agent) -> [GatewayAccount] {
        accounts.filter { $0.agent == agent }
    }
}

extension GatewayState {
    init(json: JSON) {
        exists = json.bool("exists")
        name = json.optionalString("name")
        key = json.optionalString("key")
        connection = Connection(rawValue: json.string("connection")) ?? .off
        statusText = json.string("status_text")
        error = json.optionalString("error")
        version = json.optionalString("version")
        accounts = json.objects("accounts").map(GatewayAccount.init)
        capture = GatewayCapture(json: json.object("capture") ?? [:])
        signIn = json.object("sign_in").map(GatewaySignIn.init)
        logHead = (json["log_head"] as? NSNumber)?.intValue
        mcpServers = (json["mcp_servers"] as? [JSON])?.map(McpServerRow.init)
        callsHead = (json["calls_head"] as? NSNumber)?.intValue
        known = json.objects("known").map(KnownMcpServer.init)
    }
}

/// An account of the gateway's pool: a Claude organisation or a Codex account.
struct GatewayAccount: Equatable, Identifiable {
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

/// What the gateway keeps in full of the requests it passes on.
struct GatewayCapture: Equatable {
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

/// An account's sign-in under way on the gateway.
struct GatewaySignIn: Equatable, Identifiable {
    let agent: Agent
    /// "Sign In to Claude Code", "Sign In to Claude Code Again".
    let title: String
    /// The id of the account it signs in again.
    let again: String?
    /// Who signs in again: the account's email, or its name.
    let account: String?
    let url: URL?
    /// The code to enter on the page (Codex).
    let userCode: String?
    /// The page shows a code to paste back (Claude).
    let needsCode: Bool
    /// The gateway is finishing it.
    let waiting: Bool
    let text: String
    let error: String?

    init(json: JSON) {
        agent = Agent(rawValue: json.string("agent")) ?? .claude
        title = json.string("title")
        again = json.optionalString("again")
        account = json.optionalString("account")
        url = URL(string: json.string("url"))
        userCode = json.optionalString("user_code")
        needsCode = json.bool("needs_code")
        waiting = json.bool("waiting")
        text = json.string("text")
        error = json.optionalString("error")
    }

    var id: String { agent.rawValue }
}

/// How to run a new gateway, and the command with the code that links it to the account.
struct GatewayEnroll {
    let code: String
    let expiresAt: Double
    let image: String
    let command: String
    let spans: [Int]
    let steps: [String]

    init(json: JSON) {
        code = json.string("code")
        expiresAt = json.double("expires_at")
        image = json.string("image")
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

/// A request in the gateway's log.
struct GatewayLogRow: Equatable, Identifiable {
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
struct GatewayLogPage {
    let rows: [GatewayLogRow]
    let more: Bool

    init(json: JSON) {
        rows = json.objects("rows").map(GatewayLogRow.init)
        more = json.bool("more")
    }
}

/// A captured request in full: the request and the response, each its headers then its body,
/// laid out off the main thread.
struct GatewayCaptured {
    let row: GatewayLogRow
    let request: CodeDocument
    let response: CodeDocument

    /// Nil when the gateway no longer has it.
    init?(json: JSON) {
        guard let captured = json.object("request") else { return nil }
        let row = GatewayLogRow(json: captured.object("row") ?? [:])
        let part = { (name: String, headers: String, body: String) in
            let headers = (captured[headers] as? [[String]] ?? []).map { $0.joined(separator: ": ") }
            let files = [CodeFile(path: "Headers", lines: headers), CodeFile(path: "Body", lines: captured.string(body).components(separatedBy: "\n"))]
            return CodeDocument(id: "gateway-\(name)-\(row.id)", files: files, truncated: false, headed: true)
        }
        self.row = row
        request = part("request", "request_headers", "request_body")
        response = part("response", "response_headers", "response_body")
    }
}

/// A captured request the page shows, by its entry in the log.
struct GatewayCapturedEntry: Identifiable {
    let id: Int
}

/// The gateway's request log as the page pages through it: all of it or only what failed.
@Observable
final class GatewayLog {
    private(set) var rows: [GatewayLogRow] = []
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
        store.loadGatewayLog(before: nil, errorsOnly: errorsOnly) { [weak self] result in
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
        store.loadGatewayLog(before: before, errorsOnly: errorsOnly) { [weak self] result in
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
    /// While a view shows the gateway, the core stays connected to it and follows its state.
    func watchGateway(_ on: Bool) {
        let before = gatewayWatchers
        gatewayWatchers = max(0, gatewayWatchers + (on ? 1 : -1))
        guard (before == 0) != (gatewayWatchers == 0) else { return }
        core.send("watch_gateway", ["on": gatewayWatchers > 0])
    }

    /// Makes the code a new gateway joins the account with.
    func addGateway(reply: @escaping (Result<GatewayEnroll, CoreBridge.CoreError>) -> Void) {
        core.send("add_gateway") { result in reply(result.map { GatewayEnroll(json: $0) }) }
    }

    /// While the account is watched, a gateway that joins it shows up at once.
    func watchAccount(_ on: Bool) {
        core.send("watch_servers", ["on": on])
    }

    func removeGateway() {
        gatewayCommand("remove_gateway")
    }

    /// Starts signing an account in on the gateway. Its sheet shows the page to open.
    func startGatewaySignIn(_ agent: Agent, again account: String? = nil) {
        guard gatewaySignInStarting == nil else { return }
        gatewaySignInStarting = agent
        var fields: [String: Any] = ["agent": agent.rawValue]
        fields["account"] = account
        core.send("gateway_sign_in", fields) { [weak self] result in
            self?.gatewaySignInStarting = nil
            guard case .failure(let error) = result else { return }
            self?.alert(error)
        }
    }

    /// Finishes Claude's sign-in with the code its page showed. Its error shows in the sign-in.
    func finishGatewaySignIn(code: String) {
        core.send("gateway_finish_sign_in", ["code": code.trimmingCharacters(in: .whitespacesAndNewlines)])
    }

    func cancelGatewaySignIn() {
        core.send("gateway_cancel_sign_in")
    }

    func renameGatewayAccount(_ account: GatewayAccount, to name: String) {
        updateGatewayAccount(account, ["type": "name", "name": name.trimmingCharacters(in: .whitespaces)])
    }

    /// `priority` counts from 0, which is used first.
    func setGatewayAccountPriority(_ account: GatewayAccount, to priority: Int) {
        updateGatewayAccount(account, ["type": "priority", "priority": priority])
    }

    func setGatewayAccount(_ account: GatewayAccount, enabled: Bool) {
        updateGatewayAccount(account, ["type": "enabled", "enabled": enabled])
    }

    func removeGatewayAccount(_ account: GatewayAccount) {
        gatewayCommand("gateway_remove_account", ["account": account.id])
    }

    func setGatewayCapture(_ kind: GatewayCapture.Kind) {
        gatewayCommand("gateway_capture", ["capture": GatewayCapture.command(kind)])
    }

    func loadGatewayLog(before: Int?, errorsOnly: Bool, reply: @escaping (Result<GatewayLogPage, CoreBridge.CoreError>) -> Void) {
        var command: JSON = ["errors_only": errorsOnly]
        if let before { command["before"] = before }
        core.send("gateway_log", command, read: GatewayLogPage.init, reply: reply)
    }

    /// A captured request in full, or nil when the gateway no longer has it.
    func loadGatewayCaptured(_ entry: Int, reply: @escaping (Result<GatewayCaptured?, CoreBridge.CoreError>) -> Void) {
        core.send("gateway_captured", ["entry": entry], read: GatewayCaptured.init, reply: reply)
    }

    private func updateGatewayAccount(_ account: GatewayAccount, _ change: JSON) {
        gatewayCommand("gateway_update_account", ["account": account.id, "change": change])
    }

    private func gatewayCommand(_ type: String, _ fields: JSON = [:]) {
        core.send(type, fields) { [weak self] result in
            if case .failure(let error) = result { self?.alert(error) }
        }
    }
}
