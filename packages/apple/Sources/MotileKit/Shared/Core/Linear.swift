import Foundation
import Observation
import SwiftUI

/// A Linear workspace a server is connected to, and the user who connected it.
struct LinearConnection: Equatable, Identifiable {
    let id: String
    let workspace: String
    let user: String

    init(json: JSON) {
        id = json.string("id")
        workspace = json.string("workspace")
        user = json.string("user")
    }
}

/// A status a team's issues can have.
struct LinearState: Equatable, Identifiable {
    let id: String
    let name: String
    let kind: String
    let color: Color

    init(json: JSON) {
        id = json.string("id")
        name = json.string("name")
        kind = json.string("kind")
        color = PullRequestPage.color(hex: json.string("color"))
    }

    var symbol: Symbol {
        switch kind {
        case "triage": .circleDotDashed
        case "backlog": .circleDashed
        case "started": .circleDot
        case "completed": .circleCheck
        case "canceled": .circleX
        default: .circle
        }
    }

    /// The kinds of status in the order an issue goes through them, with what Linear calls them.
    static let kinds: [(kind: String, name: String)] = [
        ("triage", "Triage"), ("backlog", "Backlog"), ("unstarted", "Todo"), ("started", "In Progress"),
        ("completed", "Done"), ("canceled", "Canceled"),
    ]
}

struct LinearUser: Equatable, Identifiable {
    let id: String
    let name: String
    /// The one who connected the workspace.
    let me: Bool

    init(json: JSON) {
        id = json.string("id")
        name = json.string("name")
        me = json.bool("me")
    }
}

struct LinearTeam: Equatable, Identifiable {
    let id: String
    let name: String
    /// In the order an issue goes through them.
    let states: [LinearState]

    init(json: JSON) {
        id = json.string("id")
        name = json.string("name")
        states = json.objects("states").map(LinearState.init)
    }
}

/// The issues of one status, as the core lists them.
struct LinearGroup: Identifiable {
    let state: LinearState
    let rows: [LinearRow]

    var id: String { state.id }

    init(json: JSON) {
        state = LinearState(json: json.object("state") ?? [:])
        rows = json.objects("rows").map(LinearRow.init)
    }
}

struct LinearRow: Identifiable {
    let id: String
    let identifier: String
    let title: String
    let url: URL?
    let priority: Int
    let priorityLabel: String
    let team: String
    let assignee: String?
    let assigneeID: String?
    let initials: String?
    let labels: [(name: String, color: Color)]
    let updatedAt: Double

    init(json: JSON) {
        id = json.string("id")
        identifier = json.string("identifier")
        title = json.string("title")
        url = URL(string: json.string("url"))
        priority = json.int("priority")
        priorityLabel = json.string("priority_label")
        team = json.string("team")
        assignee = json.optionalString("assignee")
        assigneeID = json.optionalString("assignee_id")
        initials = json.optionalString("initials")
        labels = json.objects("labels").map { ($0.string("name"), PullRequestPage.color(hex: $0.string("color"))) }
        updatedAt = json.double("updated_at")
    }

    var prioritySymbol: Symbol { Self.symbol(priority: priority) }

    /// Linear's priorities in the order it offers them, with what it calls them.
    static let priorities: [(value: Int, name: String)] = [(0, "No priority"), (1, "Urgent"), (2, "High"), (3, "Medium"), (4, "Low")]

    static func symbol(priority: Int) -> Symbol {
        switch priority {
        case 1: .triangleAlert
        case 2: .signalHigh
        case 3: .signalMedium
        case 4: .signalLow
        default: .ellipsis
        }
    }
}

/// One issue as its tab shows it, set by the core.
struct LinearPage {
    struct Comment: Identifiable {
        let id: String
        let author: String
        let initials: String
        let at: Double
        let body: [PullRequestText]
    }

    let row: LinearRow
    let state: LinearState
    let description: [PullRequestText]
    let comments: [Comment]
    /// What hands the issue to an agent.
    let prompt: String

    init(json: JSON) {
        row = LinearRow(json: json.object("row") ?? [:])
        state = LinearState(json: json.object("state") ?? [:])
        description = PullRequestText.blocks(json.objects("description"))
        comments = json.objects("comments").map { comment in
            Comment(
                id: comment.string("id"), author: comment.string("author"), initials: comment.string("initials"),
                at: comment.double("at"), body: PullRequestText.blocks(comment.objects("body")))
        }
        prompt = json.string("prompt")
    }
}

/// What the Linear tab lists for a project, kept so that it opens as it was left.
struct LinearChoice: Codable, Equatable {
    var workspace: String?
    var team: String?
    var mine = true
    /// The kinds of status listed, or none for all of them.
    var states = ["unstarted", "started"]
}

/// The Linear workspaces the servers are connected to, and the issues the tab lists.
@Observable
final class Linear {
    @ObservationIgnored weak var store: AppStore?
    /// By server, as last heard.
    private var connections: [String: [LinearConnection]] = [:]
    /// The server that waits for the user to approve it at Linear.
    private(set) var connecting: String?
    var error: String?
    /// By workspace.
    private(set) var teams: [String: [LinearTeam]] = [:]
    /// Who can be assigned, by workspace.
    private(set) var users: [String: [LinearUser]] = [:]
    private(set) var issues: Loaded<[LinearGroup]> = .loading
    /// The open issues, by id.
    private(set) var pages: [String: Loaded<LinearPage>] = [:]
    /// The issues that are being changed, those commented on as `comment:` and their id, those
    /// read as `read:` and their id, `issues` while the list is read and `create` while one is
    /// filed.
    private(set) var working: Set<String> = []
    /// By project.
    private var choices: [String: LinearChoice]

    @ObservationIgnored private var session: SignInSession?
    /// What `issues` was asked with.
    @ObservationIgnored private var listed: Listed?
    @ObservationIgnored private let defaults = UserDefaults.standard

    private struct Listed: Equatable {
        let target: PanelTarget
        let choice: LinearChoice
        let search: String
    }

    init() {
        let kept = defaults.data(forKey: "linear.choices").flatMap { try? JSONDecoder().decode([String: LinearChoice].self, from: $0) }
        choices = kept ?? [:]
    }

    func connected(_ serverID: String) -> [LinearConnection] {
        connections[serverID] ?? []
    }

    /// What the tab lists for the project: what was chosen there last, in a workspace that is
    /// still connected.
    func choice(for target: PanelTarget) -> LinearChoice {
        var choice = choices[target.projectID] ?? LinearChoice()
        let connected = connected(target.serverID)
        guard !connected.contains(where: { $0.id == choice.workspace }) else { return choice }
        choice.workspace = connected.first?.id
        choice.team = nil
        return choice
    }

    func choose(for target: PanelTarget, _ change: (inout LinearChoice) -> Void) {
        var choice = choice(for: target)
        change(&choice)
        choices[target.projectID] = choice
        defaults.set(try? JSONEncoder().encode(choices), forKey: "linear.choices")
    }

    func read(_ serverID: String) {
        guard let server = store?.server(serverID), server.protocolVersion >= 12 else { return }
        if connections[serverID] == nil, let kept = defaults.array(forKey: "linear-\(serverID)") as? [JSON] {
            connections[serverID] = kept.map(LinearConnection.init)
        }
        ask(["type": "linear_status"], on: serverID)
    }

    /// Has the user pick a workspace and approve Motile at Linear in the browser, then hands
    /// what Linear sent back to the server, which alone can finish with it.
    func connect(_ serverID: String) {
        guard connecting == nil else { return }
        connecting = serverID
        error = nil
        store?.core.send("request", ["server_id": serverID, "request": ["type": "linear_connect"]]) { [weak self] result in
            guard let self else { return }
            var refusal = "The connection couldn't be started."
            if case .failure(let error) = result { refusal = error.message }
            guard case .success(let answer) = result, let url = URL(string: answer.string("url")) else {
                connecting = nil
                error = refusal
                return
            }
            let session = SignInSession()
            self.session = session
            session.start(url: url) { [weak self] callback in
                guard let self else { return }
                self.session = nil
                let sent = callback.flatMap { URLComponents(url: $0, resolvingAgainstBaseURL: false) }?.queryItems ?? []
                let value = { (name: String) in sent.first { $0.name == name }?.value }
                guard let code = value("code"), let state = value("state") else {
                    connecting = nil
                    if let refusal = value("error"), refusal != "access_denied" { error = "Linear didn't connect: \(refusal)" }
                    return
                }
                ask(["type": "linear_finish", "code": code, "state": state], on: serverID)
            }
        }
    }

    func disconnect(_ workspace: String, on serverID: String) {
        ask(["type": "linear_disconnect", "workspace": workspace], on: serverID)
    }

    /// Asks the server for the issues the project's tab lists, or for the ones that have the
    /// words of `search`, and for the workspace's teams and users when they aren't known yet.
    func load(_ target: PanelTarget, search: String = "") {
        let choice = choice(for: target)
        guard let workspace = choice.workspace else { return }
        let asked = Listed(target: target, choice: choice, search: search.trimmingCharacters(in: .whitespaces))
        if listed != asked || issues.value == nil {
            listed = asked
            issues = .loading
        }
        working.insert("issues")
        if teams[workspace] == nil { loadTeams(of: workspace, on: target.serverID) }
        // A server that doesn't read `states` yet goes by `closed`.
        let closed = choice.states.isEmpty || choice.states.contains { ["completed", "canceled"].contains($0) }
        var command: JSON = [
            "server_id": target.serverID, "workspace": workspace, "mine": choice.mine, "closed": closed, "states": choice.states,
        ]
        if let team = choice.team { command["team"] = team }
        if !asked.search.isEmpty { command["search"] = asked.search }
        store?.core.send("linear_issues", command, read: { $0.objects("groups").map(LinearGroup.init) }) { [weak self] result in
            guard let self, listed == asked else { return }
            working.remove("issues")
            switch result {
            case .success(let groups):
                issues = .ready(groups)
                error = nil
            case .failure(let failure):
                if issues.value == nil { issues = .failed(failure.message) } else { error = failure.message }
                // Linear may have ended the connection.
                ask(["type": "linear_status"], on: target.serverID)
            }
        }
    }

    /// Reads the issue for its tab, and with `comment` says that on it first.
    func loadIssue(_ id: String, of workspace: String, on serverID: String, comment: String? = nil, done: (() -> Void)? = nil) {
        var command: JSON = ["server_id": serverID, "workspace": workspace, "issue": id]
        if let comment {
            command["comment"] = comment
            working.insert("comment:\(id)")
        }
        if pages[id]?.value == nil { pages[id] = .loading }
        working.insert("read:\(id)")
        store?.core.send("linear_issue", command, read: { LinearPage(json: $0.object("page") ?? [:]) }) { [weak self] result in
            guard let self else { return }
            working.remove("read:\(id)")
            if comment != nil { working.remove("comment:\(id)") }
            switch result {
            case .success(let page):
                pages[id] = .ready(page)
                error = nil
                done?()
            case .failure(let failure):
                if pages[id]?.value == nil { pages[id] = .failed(failure.message) } else { error = failure.message }
            }
        }
    }

    /// Changes the issue's status, assignee or priority at Linear, then reads what shows it again.
    func change(_ id: String, _ change: JSON, of workspace: String, on serverID: String) {
        guard !working.contains(id) else { return }
        working.insert(id)
        let request: JSON = ["type": "linear_update", "workspace": workspace, "issue": id, "change": change]
        store?.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard let self else { return }
            working.remove(id)
            if case .failure(let failure) = result { error = failure.message }
            refresh(issue: id, of: workspace, on: serverID)
        }
    }

    /// Files an issue and answers with its id and identifier.
    func create(_ issue: JSON, in workspace: String, on serverID: String, done: @escaping (String, String) -> Void) {
        guard !working.contains("create") else { return }
        working.insert("create")
        let request: JSON = ["type": "linear_create", "workspace": workspace, "issue": issue]
        store?.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard let self else { return }
            working.remove("create")
            switch result {
            case .success(let answer):
                let filed = answer.object("issue") ?? [:]
                error = nil
                refresh(issue: nil, of: workspace, on: serverID)
                done(filed.string("id"), filed.string("identifier"))
            case .failure(let failure):
                error = failure.message
            }
        }
    }

    /// Puts what the issue asks for in the open draft's composer, or from a thread in that of a
    /// new draft. An issue nobody works on yet is taken:
    /// assigned to the user and moved to the first status of work.
    func work(on page: LinearPage, of workspace: String, in target: PanelTarget) {
        var taking: JSON = [:]
        let states = teams[workspace]?.first { $0.id == page.row.team }?.states ?? []
        if ["triage", "backlog", "unstarted"].contains(page.state.kind), let started = states.first(where: { $0.kind == "started" }) {
            taking["state"] = started.id
        }
        if page.row.assigneeID == nil, let me = users[workspace]?.first(where: \.me) {
            taking["assignee"] = me.id
        }
        if !taking.isEmpty { change(page.row.id, taking, of: workspace, on: target.serverID) }
        store?.startWork(page.prompt, in: target.projectID)
    }

    private func refresh(issue id: String?, of workspace: String, on serverID: String) {
        if let id, pages[id] != nil { loadIssue(id, of: workspace, on: serverID) }
        if let listed, listed.target.serverID == serverID { load(listed.target, search: listed.search) }
    }

    private func loadTeams(of workspace: String, on serverID: String) {
        let request: JSON = ["type": "linear_teams", "workspace": workspace]
        store?.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard case .success(let answer) = result else { return }
            self?.teams[workspace] = answer.objects("teams").map(LinearTeam.init)
            self?.users[workspace] = answer.objects("users").map(LinearUser.init)
        }
    }

    /// The teams and users of the workspace, asked for when they aren't known yet.
    func needTeams(of workspace: String, on serverID: String) {
        guard teams[workspace] == nil else { return }
        loadTeams(of: workspace, on: serverID)
    }

    /// Sends a request that `Linear` answers, with the server's workspaces.
    private func ask(_ request: JSON, on serverID: String) {
        let finishing = request.string("type") == "linear_finish"
        store?.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard let self else { return }
            if finishing { connecting = nil }
            switch result {
            case .success(let answer):
                let before = Set(connected(serverID).map(\.id))
                let heard = answer.objects("connections")
                connections[serverID] = heard.map(LinearConnection.init)
                defaults.set(heard, forKey: "linear-\(serverID)")
                guard finishing, let target = store?.panelTarget else { return }
                let added = connected(serverID).first { !before.contains($0.id) }
                guard let added else { return }
                choose(for: target) { choice in
                    choice.workspace = added.id
                    choice.team = nil
                }
            case .failure(let failure):
                guard request.string("type") != "linear_status" else { return }
                error = failure.message
            }
        }
    }
}
