import Foundation
import Observation
import SwiftUI

/// An MCP the account's gateway holds the login of, which every agent is given, as its card shows it.
struct McpServerRow: Equatable, Identifiable {
    enum State: String {
        case ready, off, failing
        case signInAgain = "sign_in_again"
    }

    enum Auth: String {
        case none, key
        case signIn = "sign_in"
    }

    let id: String
    let slug: String
    let name: String
    let url: String
    let host: String
    let monogram: String
    let tint: String
    let enabled: Bool
    let auth: Auth
    let state: State
    /// "Signed in · mcp.linear.app", "Off · agents don't get it", what went wrong.
    let statusText: String
    /// "30 of 32 tools on".
    let toolsText: String
    let tools: [McpServerTool]
    /// Some tools write and some only read, so "Read Only" changes something.
    let hasReadOnly: Bool
    /// How agents name one of its tools: `mcp__linear__create_issue`.
    let example: String

    init(json: JSON) {
        id = json.string("id")
        slug = json.string("slug")
        name = json.string("name")
        url = json.string("url")
        host = json.string("host")
        monogram = json.string("monogram")
        tint = json.string("tint")
        enabled = json.bool("enabled")
        auth = Auth(rawValue: json.string("auth")) ?? .none
        state = State(rawValue: json.string("state")) ?? .ready
        statusText = json.string("status_text")
        toolsText = json.string("tools_text")
        tools = json.objects("tools").map(McpServerTool.init)
        hasReadOnly = json.bool("has_read_only")
        example = json.string("example")
    }

    /// Its login stopped working or it fails, which its status says in warning.
    var troubled: Bool { state == .signInAgain || state == .failing }

    /// How agents name its tools once it is called `slug`.
    func example(as slug: String) -> String {
        let tool = example.components(separatedBy: "__").last ?? "tool"
        return "mcp__\(slug.isEmpty ? self.slug : slug)__\(tool)"
    }
}

struct McpServerTool: Equatable, Identifiable {
    let name: String
    let title: String
    let summary: String?
    let writes: Bool
    let enabled: Bool

    var id: String { name }

    init(json: JSON) {
        name = json.string("name")
        title = json.string("title")
        summary = json.optionalString("summary")
        writes = json.bool("writes")
        enabled = json.bool("enabled")
    }
}

/// An MCP the add sheet offers by name.
struct KnownMcpServer: Equatable, Identifiable {
    let name: String
    let url: String
    let host: String
    let monogram: String
    let tint: String
    let keywords: String

    var id: String { url }

    init(json: JSON) {
        name = json.string("name")
        url = json.string("url")
        host = json.string("host")
        monogram = json.string("monogram")
        tint = json.string("tint")
        keywords = json.string("keywords")
    }

    func matches(_ search: String) -> Bool {
        let words = search.lowercased().split(separator: " ")
        let text = "\(name) \(host) \(keywords)".lowercased()
        return words.allSatisfy { text.contains($0) }
    }
}

/// A call an agent made to an MCP server's tool, or one tried in Settings.
struct McpServerCallRow: Equatable, Identifiable {
    let id: Int
    let time: String
    let mcpServer: String
    let monogram: String
    let tint: String
    let tool: String
    /// "Fix the sync race on studio", "Tried in Settings".
    let source: String
    let latency: String
    let ok: Bool
    let error: String?

    init(json: JSON) {
        id = json.int("id")
        time = json.string("time")
        mcpServer = json.string("mcp_server")
        monogram = json.string("monogram")
        tint = json.string("tint")
        tool = json.string("tool")
        source = json.string("source")
        latency = json.string("latency")
        ok = json.bool("ok")
        error = json.optionalString("error")
    }

    /// "14:32:05 · Linear · create_issue · Fix the build on studio · 412 ms".
    var summary: String {
        [time, mcpServer, tool, source, latency].joined(separator: " · ")
    }
}

struct McpServerCallPage {
    let rows: [McpServerCallRow]
    let more: Bool

    init(json: JSON) {
        rows = json.objects("rows").map(McpServerCallRow.init)
        more = json.bool("more")
    }
}

/// A call in full: what the agent sent and what came back.
struct McpServerCallView {
    let row: McpServerCallRow
    let arguments: String
    let result: String
    let error: String?

    /// Nil when the gateway no longer has it.
    init?(json: JSON) {
        guard let call = json.object("call") else { return nil }
        row = McpServerCallRow(json: call.object("row") ?? [:])
        arguments = call.string("arguments")
        result = call.string("result")
        error = call.optionalString("error")
    }
}

/// A field of the form a tool is tried with.
struct McpServerField: Equatable, Identifiable {
    enum Kind: String {
        case text, number, choice, json
        case toggle = "switch"
    }

    let name: String
    let description: String?
    let kind: Kind
    let required: Bool
    let choices: [String]
    /// What it starts with.
    let value: String

    var id: String { name }

    init(json: JSON) {
        name = json.string("name")
        description = json.optionalString("description")
        kind = Kind(rawValue: json.string("kind")) ?? .text
        required = json.bool("required")
        choices = json.strings("choices")
        value = json.string("value")
    }
}

/// What a tried tool answered: its text, the whole answer as JSON, and what went wrong.
struct McpServerAnswer: Equatable {
    let text: String
    let json: String
    let error: String?

    init(json: JSON) {
        text = json.string("text")
        self.json = json.string("json")
        error = json.optionalString("error")
    }
}

/// The calls agents made to the MCP servers, paged back from the newest: all of them or only those
/// that failed.
@Observable
final class McpServerCalls {
    private(set) var rows: [McpServerCallRow] = []
    private(set) var more = false
    private(set) var loading = false
    private(set) var error: String?
    var errorsOnly = false {
        didSet {
            guard errorsOnly != oldValue else { return }
            rows = []
            more = false
            load(before: nil)
        }
    }
    @ObservationIgnored private weak var store: AppStore?
    @ObservationIgnored private var asked = 0

    func start(_ store: AppStore) {
        self.store = store
        load(before: nil)
    }

    func loadMore() {
        guard more, !loading, let last = rows.last else { return }
        load(before: last.id)
    }

    /// Puts the calls that came since the newest row on top, or starts over when more came than a
    /// page holds.
    func refresh() {
        guard let store, !loading else { return }
        let number = asked
        store.loadMcpServerCalls(before: nil, errorsOnly: errorsOnly) { [weak self] result in
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

    /// Said where there are no rows.
    var emptyText: String? {
        guard rows.isEmpty, !loading else { return nil }
        if let error { return error }
        return errorsOnly ? "No errors" : "No calls yet"
    }

    private func load(before: Int?) {
        guard let store else { return }
        asked += 1
        let number = asked
        loading = true
        error = nil
        store.loadMcpServerCalls(before: before, errorsOnly: errorsOnly) { [weak self] result in
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

/// Adding an MCP server, step by step: the gateway is asked to add it, and it is added at once, signs in
/// in the browser, or wants a key.
@Observable
final class McpServerAdder {
    enum Step: Equatable {
        case pick
        case signingIn(name: String)
        /// `name` is what the sheet calls it; `given` the known MCP server's name, sent again with the key.
        case key(name: String, url: String, given: String?)
        case added(id: String)
    }

    private(set) var step = Step.pick
    /// What is being added: a known MCP server's url, or the address typed.
    private(set) var adding: String?
    private(set) var error: String?
    @ObservationIgnored private weak var store: AppStore?
    @ObservationIgnored private var session: SignInSession?

    func start(_ store: AppStore) {
        self.store = store
    }

    func add(url: String, name: String?, header: String = "", key: String = "") {
        guard let store, adding == nil else { return }
        adding = url
        error = nil
        var command: JSON = ["url": url, "header": header, "key": key]
        if let name { command["name"] = name }
        store.core.send("mcp_server_add", command) { [weak self] result in
            guard let self else { return }
            adding = nil
            answered(result, name: name, url: url)
        }
    }

    /// Back to picking another.
    func back() {
        session = nil
        adding = nil
        error = nil
        step = .pick
    }

    private func answered(_ result: Result<JSON, CoreBridge.CoreError>, name given: String?, url: String) {
        let name = given ?? Self.hostName(url)
        switch result {
        case .failure(let failure):
            error = failure.message
        case .success(let answer):
            if let id = answer.optionalString("added") {
                step = .added(id: id)
            } else if let signIn = answer.object("sign_in"), let page = URL(string: signIn.string("url")) {
                signInInBrowser(page, name: name)
            } else if answer.bool("needs_key") {
                step = .key(name: name, url: url, given: given)
            }
        }
    }

    private func signInInBrowser(_ page: URL, name: String) {
        step = .signingIn(name: name)
        let session = SignInSession()
        self.session = session
        session.start(url: page) { [weak self] callback in
            guard let self, self.session === session else { return }
            self.session = nil
            guard let callback, let store else { return back() }
            store.core.send("mcp_server_finish_sign_in", ["address": callback.absoluteString]) { [weak self] result in
                guard let self else { return }
                switch result {
                case .success(let answer):
                    step = answer.optionalString("added").map { .added(id: $0) } ?? .pick
                case .failure(let failure):
                    step = .pick
                    error = failure.message
                }
            }
        }
    }

    /// "Linear" for `https://mcp.linear.app/mcp`, until the gateway names it; an address by itself.
    private static func hostName(_ url: String) -> String {
        let host = URL(string: url)?.host() ?? url
        guard host.contains(where: \.isLetter), host != "localhost" else { return host }
        let skipped: Set = ["mcp", "api", "www", "server", "app"]
        let labels = host.split(separator: ".").map(String.init)
        let named = labels.dropLast().first { !skipped.contains($0) } ?? labels.first ?? host
        return named.prefix(1).uppercased() + named.dropFirst()
    }
}

extension AppStore {
    /// The MCP server of the gateway by its id, as the page knows it.
    func mcpServer(_ id: String) -> McpServerRow? {
        gateway.mcpServers?.first { $0.id == id }
    }

    /// Signs an MCP server in again in the browser; its card shows it pending meanwhile.
    func signMcpServerInAgain(_ mcpServer: McpServerRow) {
        guard mcpServerSigningIn == nil else { return }
        mcpServerSigningIn = mcpServer.id
        core.send("mcp_server_sign_in", ["mcp_server": mcpServer.id]) { [weak self] result in
            guard let self else { return }
            guard case .success(let answer) = result, let signIn = answer.object("sign_in"), let page = URL(string: signIn.string("url")) else {
                mcpServerSigningIn = nil
                if case .failure(let error) = result { alert(error) }
                return
            }
            let session = SignInSession()
            mcpServerSession = session
            session.start(url: page) { [weak self] callback in
                guard let self else { return }
                mcpServerSession = nil
                guard let callback else {
                    mcpServerSigningIn = nil
                    return
                }
                core.send("mcp_server_finish_sign_in", ["address": callback.absoluteString]) { [weak self] result in
                    self?.mcpServerSigningIn = nil
                    if case .failure(let error) = result { self?.alert(error) }
                }
            }
        }
    }

    func setMcpServer(_ mcpServer: McpServerRow, enabled: Bool) {
        updateMcpServer(mcpServer.id, ["type": "enabled", "enabled": enabled])
    }

    func setMcpServerTool(_ mcpServer: McpServerRow, _ tool: McpServerTool, enabled: Bool) {
        updateMcpServer(mcpServer.id, ["type": "tool", "name": tool.name, "enabled": enabled])
    }

    /// Turns off every tool that writes, or every tool on.
    func setMcpServerTools(_ mcpServer: McpServerRow, readOnly: Bool) {
        updateMcpServer(mcpServer.id, ["type": readOnly ? "read_only" : "all_tools"])
    }

    func renameMcpServer(_ mcpServer: McpServerRow, name: String, slug: String) {
        let name = name.trimmingCharacters(in: .whitespaces)
        let slug = slug.trimmingCharacters(in: .whitespaces)
        if !name.isEmpty, name != mcpServer.name {
            updateMcpServer(mcpServer.id, ["type": "name", "name": name])
        }
        if !slug.isEmpty, slug != mcpServer.slug {
            updateMcpServer(mcpServer.id, ["type": "slug", "slug": slug])
        }
    }

    func setMcpServerKey(_ mcpServer: McpServerRow, header: String, key: String, reply: @escaping (Result<JSON, CoreBridge.CoreError>) -> Void) {
        core.send("mcp_server_key", ["mcp_server": mcpServer.id, "header": header, "key": key], reply: reply)
    }

    func refreshMcpServer(_ mcpServer: McpServerRow) {
        mcpServerCommand("mcp_server_refresh", ["mcp_server": mcpServer.id])
    }

    func removeMcpServer(_ mcpServer: McpServerRow) {
        mcpServerCommand("mcp_server_remove", ["mcp_server": mcpServer.id])
    }

    func loadMcpServerCalls(before: Int?, errorsOnly: Bool, reply: @escaping (Result<McpServerCallPage, CoreBridge.CoreError>) -> Void) {
        var command: JSON = ["errors_only": errorsOnly]
        if let before { command["before"] = before }
        core.send("mcp_server_calls", command, read: McpServerCallPage.init, reply: reply)
    }

    func loadMcpServerCall(_ id: Int, reply: @escaping (Result<McpServerCallView?, CoreBridge.CoreError>) -> Void) {
        core.send("mcp_server_call", ["call": id], read: McpServerCallView.init, reply: reply)
    }

    func loadMcpServerFields(_ mcpServer: String, tool: String, reply: @escaping (Result<[McpServerField], CoreBridge.CoreError>) -> Void) {
        core.send("mcp_server_fields", ["mcp_server": mcpServer, "tool": tool], read: { $0.objects("fields").map(McpServerField.init) }, reply: reply)
    }

    func tryMcpServerTool(_ mcpServer: String, tool: String, values: [String: String], reply: @escaping (Result<McpServerAnswer, CoreBridge.CoreError>) -> Void) {
        core.send("mcp_server_try", ["mcp_server": mcpServer, "tool": tool, "values": values], read: McpServerAnswer.init, reply: reply)
    }

    private func updateMcpServer(_ id: String, _ change: JSON) {
        mcpServerCommand("mcp_server_update", ["mcp_server": id, "change": change])
    }

    private func mcpServerCommand(_ type: String, _ fields: JSON) {
        core.send(type, fields) { [weak self] result in
            if case .failure(let error) = result { self?.alert(error) }
        }
    }
}

extension Color {
    /// An MCP server's tile colour, by the chart token its row names.
    static func mcpServerTint(_ token: String) -> Color {
        Theme.named(token).map { Color(platform: $0) } ?? .themeChart1
    }
}
