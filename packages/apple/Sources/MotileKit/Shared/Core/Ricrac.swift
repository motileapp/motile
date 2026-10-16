import Foundation
import Observation
import SwiftUI

/// A Ricrac workspace a server is connected to, and the user who connected it.
struct RicracConnection: Equatable, Identifiable {
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
struct RicracState: Equatable, Identifiable {
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

    /// The kinds of status in the order an issue goes through them, with what Ricrac calls them.
    static let kinds: [(kind: String, name: String)] = [
        ("triage", "Triage"), ("backlog", "Backlog"), ("unstarted", "Todo"), ("started", "In Progress"),
        ("completed", "Done"), ("canceled", "Canceled"),
    ]
}

struct RicracUser: Equatable, Identifiable {
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

struct RicracTeam: Equatable, Identifiable {
    let id: String
    let name: String
    /// In the order an issue goes through them.
    let states: [RicracState]

    init(json: JSON) {
        id = json.string("id")
        name = json.string("name")
        states = json.objects("states").map(RicracState.init)
    }
}

/// The issues of one status, as the core lists them.
struct RicracGroup: Equatable, Identifiable {
    let state: RicracState
    let rows: [RicracRow]

    var id: String { state.id }

    init(json: JSON) {
        state = RicracState(json: json.object("state") ?? [:])
        rows = json.objects("rows").map(RicracRow.init)
    }
}

struct RicracRow: Equatable, Identifiable {
    struct Label: Equatable {
        let name: String
        let color: Color
    }

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
    let labels: [Label]
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
        labels = json.objects("labels").map { Label(name: $0.string("name"), color: PullRequestPage.color(hex: $0.string("color"))) }
        updatedAt = json.double("updated_at")
    }

    var prioritySymbol: Symbol { Self.symbol(priority: priority) }

    /// Ricrac's priorities in the order it offers them, with what it calls them.
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
struct RicracPage {
    struct Comment: Identifiable {
        let id: String
        let author: String
        let initials: String
        let at: Double
        let body: [PullRequestText]
    }

    let row: RicracRow
    let state: RicracState
    let description: [PullRequestText]
    let comments: [Comment]
    /// What hands the issue to an agent.
    let prompt: String

    init(json: JSON) {
        row = RicracRow(json: json.object("row") ?? [:])
        state = RicracState(json: json.object("state") ?? [:])
        description = PullRequestText.blocks(json.objects("description"))
        comments = json.objects("comments").map { comment in
            Comment(
                id: comment.string("id"), author: comment.string("author"), initials: comment.string("initials"),
                at: comment.double("at"), body: PullRequestText.blocks(comment.objects("body")))
        }
        prompt = json.string("prompt")
    }
}

/// The workspace and team the Ricrac tab lists for a project.
struct RicracPick: Codable, Equatable {
    var workspace: String?
    var team: String?
}

/// How the Ricrac tab lists issues, the same in every project.
struct RicracFilter: Codable, Equatable {
    var mine = true
    /// The kinds of status listed, or none for all of them.
    var states = ["unstarted", "started"]
    /// The statuses folded up, by id.
    var folded: Set<String> = []
}

/// What the Ricrac tab lists for a project.
struct RicracChoice: Equatable {
    var workspace: String?
    var team: String?
    var mine: Bool
    var states: [String]
}

/// The Ricrac workspaces the servers are connected to, and the issues the tab lists.
@Observable
final class Ricrac {
    @ObservationIgnored weak var store: AppStore?
    /// By server, as last heard.
    private var connections: [String: [RicracConnection]] = [:]
    /// The server that waits for the user to approve it at Ricrac.
    private(set) var connecting: String?
    var error: String?
    /// By workspace.
    private(set) var teams: [String: [RicracTeam]] = [:]
    /// Who can be assigned, by workspace.
    private(set) var users: [String: [RicracUser]] = [:]
    /// The lists read so far, by what asked for them, so that a tab opens as it was left.
    private var lists: [Query: Loaded<[RicracGroup]>] = [:]
    /// The open issues, by id.
    private(set) var pages: [String: Loaded<RicracPage>] = [:]
    /// The issues that are being changed, those commented on as `comment:` and their id, those
    /// read as `read:` and their id, `issues` while the list is read and `create` while one is
    /// filed.
    private(set) var working: Set<String> = []
    /// By project.
    private var picks: [String: RicracPick]
    /// What was picked last, which a project that has none of its own starts from.
    private var lastPick: RicracPick?
    private(set) var filter: RicracFilter
    /// What the tab searches for, by project, while the client runs.
    var searches: [String: String] = [:]

    @ObservationIgnored private var session: SignInSession?
    /// What the tab lists now.
    private var listed: Query?
    @ObservationIgnored private let defaults = UserDefaults.standard

    /// What a list is asked with. Two threads of one project ask the same.
    private struct Query: Hashable {
        let serverID: String
        let workspace: String
        let team: String?
        let mine: Bool
        let states: [String]
        let search: String
    }

    /// The issues the tab lists.
    var issues: Loaded<[RicracGroup]> {
        guard let listed else { return .loading }
        return lists[listed] ?? .loading
    }

    init() {
        picks = Self.kept([String: RicracPick].self, "ricrac.choices") ?? [:]
        lastPick = Self.kept(RicracPick.self, "ricrac.last")
        filter = Self.kept(RicracFilter.self, "ricrac.filter") ?? RicracFilter()
    }

    private static func kept<Value: Decodable>(_ type: Value.Type, _ key: String) -> Value? {
        UserDefaults.standard.data(forKey: key).flatMap { try? JSONDecoder().decode(type, from: $0) }
    }

    private func keep(_ value: some Encodable, _ key: String) {
        defaults.set(try? JSONEncoder().encode(value), forKey: key)
    }

    func connected(_ serverID: String) -> [RicracConnection] {
        connections[serverID] ?? []
    }

    func choice(for target: PanelTarget) -> RicracChoice {
        let pick = pick(for: target)
        return RicracChoice(workspace: pick.workspace, team: pick.team, mine: filter.mine, states: filter.states)
    }

    /// What was picked for the project last, or else for any project, in a workspace the server
    /// is still connected to.
    private func pick(for target: PanelTarget) -> RicracPick {
        let connected = connected(target.serverID)
        let kept = [picks[target.projectID], lastPick].compactMap { $0 }
        guard let pick = kept.first(where: { pick in connected.contains { $0.id == pick.workspace } }) else {
            return RicracPick(workspace: connected.first?.id)
        }
        return pick
    }

    func pick(_ workspace: String?, team: String? = nil, for target: PanelTarget) {
        let pick = RicracPick(workspace: workspace, team: team)
        picks[target.projectID] = pick
        lastPick = pick
        keep(picks, "ricrac.choices")
        keep(pick, "ricrac.last")
    }

    func changeFilter(_ change: (inout RicracFilter) -> Void) {
        change(&filter)
        keep(filter, "ricrac.filter")
    }

    func fold(_ state: String) {
        changeFilter { filter in
            guard filter.folded.remove(state) == nil else { return }
            filter.folded.insert(state)
        }
    }

    func read(_ serverID: String) {
        guard let server = store?.server(serverID), server.protocolVersion >= 29 else { return }
        if connections[serverID] == nil, let kept = defaults.array(forKey: "ricrac-\(serverID)") as? [JSON] {
            connections[serverID] = kept.map(RicracConnection.init)
        }
        ask(["type": "ricrac_status"], on: serverID)
    }

    /// Has the user pick a workspace and approve Motile at Ricrac in the browser, then hands
    /// what Ricrac sent back to the server, which alone can finish with it.
    func connect(_ serverID: String) {
        guard connecting == nil else { return }
        connecting = serverID
        error = nil
        store?.core.send("request", ["server_id": serverID, "request": ["type": "ricrac_connect"]]) { [weak self] result in
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
                    if let refusal = value("error"), refusal != "access_denied" { error = "Ricrac didn't connect: \(refusal)" }
                    return
                }
                ask(["type": "ricrac_finish", "code": code, "state": state], on: serverID)
            }
        }
    }

    func disconnect(_ workspace: String, on serverID: String) {
        ask(["type": "ricrac_disconnect", "workspace": workspace], on: serverID)
    }

    /// Asks the server for the issues the project's tab lists, or for the ones that have the
    /// words of `search`, and for the workspace's teams and users when they aren't known yet. A
    /// list read before shows at once while it is read again.
    func load(_ target: PanelTarget, search: String = "") {
        let choice = choice(for: target)
        guard let workspace = choice.workspace else { return }
        let query = Query(
            serverID: target.serverID, workspace: workspace, team: choice.team, mine: choice.mine, states: choice.states,
            search: search.trimmingCharacters(in: .whitespaces))
        if let listed, listed != query, !listed.search.isEmpty { lists[listed] = nil }
        listed = query
        if teams[workspace] == nil { loadTeams(of: workspace, on: target.serverID) }
        fetch(query)
    }

    private func fetch(_ query: Query) {
        if lists[query] == nil { lists[query] = .loading }
        working.insert("issues")
        // A server that doesn't read `states` yet goes by `closed`.
        let closed = query.states.isEmpty || query.states.contains { ["completed", "canceled"].contains($0) }
        var command: JSON = [
            "server_id": query.serverID, "workspace": query.workspace, "mine": query.mine, "closed": closed, "states": query.states,
        ]
        if let team = query.team { command["team"] = team }
        if !query.search.isEmpty { command["search"] = query.search }
        store?.core.send("ricrac_issues", command, read: { $0.objects("groups").map(RicracGroup.init) }) { [weak self] result in
            guard let self else { return }
            let shown = listed == query
            if shown { working.remove("issues") }
            switch result {
            case .success(let groups):
                lists[query] = .ready(groups)
                if shown { error = nil }
            case .failure(let failure):
                guard shown else { return }
                if lists[query]?.value == nil { lists[query] = .failed(failure.message) } else { error = failure.message }
                // Ricrac may have ended the connection.
                ask(["type": "ricrac_status"], on: query.serverID)
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
        store?.core.send("ricrac_issue", command, read: { RicracPage(json: $0.object("page") ?? [:]) }) { [weak self] result in
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

    /// Changes the issue's status, assignee or priority at Ricrac, then reads what shows it again.
    func change(_ id: String, _ change: JSON, of workspace: String, on serverID: String) {
        guard !working.contains(id) else { return }
        working.insert(id)
        let request: JSON = ["type": "ricrac_update", "workspace": workspace, "issue": id, "change": change]
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
        let request: JSON = ["type": "ricrac_create", "workspace": workspace, "issue": issue]
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
    func work(on page: RicracPage, of workspace: String, in target: PanelTarget) {
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
        if let listed, listed.serverID == serverID { fetch(listed) }
    }

    private func loadTeams(of workspace: String, on serverID: String) {
        let request: JSON = ["type": "ricrac_teams", "workspace": workspace]
        store?.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard case .success(let answer) = result else { return }
            self?.teams[workspace] = answer.objects("teams").map(RicracTeam.init)
            self?.users[workspace] = answer.objects("users").map(RicracUser.init)
        }
    }

    /// The teams and users of the workspace, asked for when they aren't known yet.
    func needTeams(of workspace: String, on serverID: String) {
        guard teams[workspace] == nil else { return }
        loadTeams(of: workspace, on: serverID)
    }

    /// Sends a request that `Ricrac` answers, with the server's workspaces.
    private func ask(_ request: JSON, on serverID: String) {
        let finishing = request.string("type") == "ricrac_finish"
        store?.core.send("request", ["server_id": serverID, "request": request]) { [weak self] result in
            guard let self else { return }
            if finishing { connecting = nil }
            switch result {
            case .success(let answer):
                let before = Set(connected(serverID).map(\.id))
                let heard = answer.objects("connections")
                connections[serverID] = heard.map(RicracConnection.init)
                defaults.set(heard, forKey: "ricrac-\(serverID)")
                guard finishing, let target = store?.panelTarget else { return }
                let added = connected(serverID).first { !before.contains($0.id) }
                guard let added else { return }
                pick(added.id, for: target)
            case .failure(let failure):
                guard request.string("type") != "ricrac_status" else { return }
                error = failure.message
            }
        }
    }
}
