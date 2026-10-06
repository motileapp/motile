import SwiftUI

/// The issues of a Linear workspace the server is connected to, or how to connect one.
struct LinearSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget

    var body: some View {
        if let reason = store.linearUnavailable {
            PanelMessage(text: reason)
        } else {
            Group {
                if store.linear.connected(target.serverID).isEmpty {
                    LinearConnect(target: target)
                } else {
                    LinearIssues(target: target)
                }
            }
            .panelTask(id: target.serverID) { store.linear.read(target.serverID) }
        }
    }
}

private struct LinearConnect: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget

    var body: some View {
        let linear = store.linear
        VStack(spacing: 16) {
            Image(.linear, size: 28)
                .foregroundStyle(Color.themeText)
            VStack(spacing: 6) {
                Text("Connect Linear")
                    .font(.ui(size: 15, weight: .semibold))
                    .foregroundStyle(Color.themeText)
                Text("See your issues and hand them to your agents. The connection is kept on your server.")
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeSecondary)
                    .multilineTextAlignment(.center)
            }
            ActionButton("Connect Linear", variant: .primary, pending: linear.connecting == target.serverID) {
                linear.connect(target.serverID)
            }
            if let error = linear.error {
                Text(error)
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeDanger)
                    .multilineTextAlignment(.center)
                    .textSelection(.enabled)
            }
        }
        .frame(maxWidth: 320)
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// A workspace's issues under their statuses, with what chooses them over the list.
private struct LinearIssues: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    @State private var search = ""
    @State private var asked = 0
    @State private var filing = false
    /// The statuses folded up, by id.
    @State private var collapsed: Set<String> = []
    @State private var offered: [Choice] = []
    /// The panel is under the width the bar and the labels beside the titles need.
    @State private var tight = false
    #if os(iOS)
    @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    /// Under this the bar is two rows.
    private static let wraps = 520 * Platform.scale

    /// A phone, or a panel too narrow for the search and the buttons beside the workspace and the
    /// team, and for the labels beside the titles.
    private var narrow: Bool {
        #if os(iOS)
        if sizeClass == .compact { return true }
        #endif
        return tight
    }

    private struct Trigger: Equatable {
        let target: PanelTarget
        let choice: LinearChoice
        let search: String
        let asked: Int
    }

    var body: some View {
        let linear = store.linear
        let choice = linear.choice(for: target)
        VStack(spacing: 0) {
            if narrow {
                PanelBar {
                    pickers(choice)
                    Spacer(minLength: 0)
                } second: {
                    tools(choice)
                }
            } else {
                PanelBar {
                    pickers(choice)
                    Spacer(minLength: 4)
                    tools(choice)
                }
            }
            if let error = linear.error {
                PanelNote(text: error)
            }
            content(choice)
                .panelTask(id: Trigger(target: target, choice: choice, search: search, asked: asked)) {
                    // Linear is asked once the typing has stopped.
                    if !search.isEmpty { try? await Task.sleep(for: .milliseconds(300)) }
                    guard !Task.isCancelled else { return }
                    linear.load(target, search: search)
                }
        }
        .onGeometryChange(for: Bool.self) { $0.size.width < Self.wraps } action: { tight = $0 }
        .sheet(isPresented: $filing) {
            if let workspace = choice.workspace {
                LinearNewIssue(target: target, workspace: workspace, team: choice.team)
                    .sheetSurface()
            }
        }
    }

    @ViewBuilder
    private func pickers(_ choice: LinearChoice) -> some View {
        let linear = store.linear
        let connected = linear.connected(target.serverID)
        let workspace = connected.first { $0.id == choice.workspace }
        let teams = linear.teams[choice.workspace ?? ""] ?? []
        ActionMenu(workspace?.workspace ?? "Linear", help: "Which workspace to show") {
            ForEach(connected) { connection in
                Toggle(connection.workspace, isOn: Binding { connection.id == choice.workspace } set: { _ in
                    linear.choose(for: target) { choice in
                        choice.workspace = connection.id
                        choice.team = nil
                    }
                })
            }
            Divider()
            Button("Add Workspace") { linear.connect(target.serverID) }
            if let workspace {
                Button("Disconnect \(workspace.workspace)", role: .destructive) { linear.disconnect(workspace.id, on: target.serverID) }
            }
        }
        .padding(.leading, -8)
        ActionMenu(teams.first { $0.id == choice.team }?.name ?? "All Teams", help: "Which team's issues to show") {
            Toggle("All Teams", isOn: Binding { choice.team == nil } set: { _ in linear.choose(for: target) { $0.team = nil } })
            ForEach(teams) { team in
                Toggle(team.name, isOn: Binding { choice.team == team.id } set: { _ in linear.choose(for: target) { $0.team = team.id } })
            }
        }
    }

    @ViewBuilder
    private func tools(_ choice: LinearChoice) -> some View {
        let linear = store.linear
        InputField("Search", text: $search, icon: .search, clearable: true)
            .frame(maxWidth: narrow ? .infinity : 180)
        ActionMenu(icon: .listFilter, help: "Which issues to show") {
            Toggle("Assigned to Me", isOn: Binding { choice.mine } set: { mine in linear.choose(for: target) { $0.mine = mine } })
            Divider()
            ForEach(LinearState.kinds, id: \.kind) { kind in
                Toggle(kind.name, isOn: Binding { choice.states.contains(kind.kind) } set: { listed in
                    linear.choose(for: target) { choice in
                        choice.states.removeAll { $0 == kind.kind }
                        if listed { choice.states.append(kind.kind) }
                    }
                })
            }
        }
        ActionButton(icon: .rotateCw, help: "Read the issues again", pending: linear.working.contains("issues")) { asked += 1 }
        ActionButton(icon: .plus, help: "New Issue") { filing = true }
    }

    @ViewBuilder
    private func content(_ choice: LinearChoice) -> some View {
        let workspace = choice.workspace ?? ""
        let teams = store.linear.teams[workspace] ?? []
        switch store.linear.issues {
        case .loading:
            PanelLoading()
        case .failed(let message):
            PanelMessage(text: message, failed: true)
        case .ready(let groups) where groups.isEmpty:
            let filtered = choice.mine || !choice.states.isEmpty
            PanelMessage(text: !search.isEmpty ? "None have “\(search)”." : filtered ? "No issues match the filter." : "No issues.")
        case .ready(let groups):
            let linear = store.linear
            let entries = groups.flatMap { group -> [LinearListEntry] in
                let folded = collapsed.contains(group.state.id)
                let header = LinearListEntry.header(state: group.state, count: group.rows.count, folded: folded)
                guard !folded else { return [header] }
                return [header] + group.rows.map { row in
                    .issue(row: row, state: group.state, states: teams.first { $0.id == row.team }?.states ?? [])
                }
            }
            LinearList(
                entries: entries, pending: linear.working, narrow: narrow, users: linear.users[workspace] ?? [], offered: $offered,
                open: { row in store.sidePanel.open(.linearIssue(workspace: workspace, id: row.id, identifier: row.identifier)) },
                fold: { id in if collapsed.contains(id) { collapsed.remove(id) } else { collapsed.insert(id) } },
                change: { row, change in linear.change(row.id, change, of: workspace, on: target.serverID) }
            )
            .choices($offered)
        }
    }
}

/// What the menus that change an issue offer.
private enum LinearChoices {
    @ViewBuilder
    static func states(_ states: [LinearState], current: String?, pick: @escaping (LinearState) -> Void) -> some View {
        ForEach(states) { state in
            Toggle(state.name, isOn: Binding { state.id == current } set: { _ in pick(state) })
        }
    }

    @ViewBuilder
    static func priorities(current: Int, pick: @escaping (Int) -> Void) -> some View {
        ForEach(LinearRow.priorities, id: \.value) { priority in
            Toggle(priority.name, isOn: Binding { priority.value == current } set: { _ in pick(priority.value) })
        }
    }

    /// `pick` is given nothing for nobody.
    @ViewBuilder
    static func assignees(_ users: [LinearUser], current: String?, pick: @escaping (String?) -> Void) -> some View {
        Toggle("Unassigned", isOn: Binding { current == nil } set: { _ in pick(nil) })
        Divider()
        ForEach(users) { user in
            Toggle(user.me ? "\(user.name) (you)" : user.name, isOn: Binding { user.id == current } set: { _ in pick(user.id) })
        }
    }
}

private struct LinearInitials: View {
    let initials: String
    let name: String

    var body: some View {
        Text(verbatim: initials)
            .font(.ui(size: 9, weight: .semibold))
            .foregroundStyle(Color.themeSecondary)
            .frame(width: scaled(18), height: scaled(18))
            .background(Color.themeBackgroundTertiary, in: Circle())
            .help(name)
    }
}

/// One issue: what it is and who has it, which menus change, what it asks for, and what was said
/// of it, with a field to say more.
struct LinearIssueSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    let workspace: String
    let id: String
    @State private var comment = ""
    @State private var asked = 0

    private struct Trigger: Equatable {
        let serverID: String
        let asked: Int
    }

    var body: some View {
        let linear = store.linear
        let page = linear.pages[id] ?? .loading
        VStack(spacing: 0) {
            PanelBar {
                Text(verbatim: page.value?.row.identifier ?? "")
                    .font(.ui(size: 12.5, weight: .medium))
                    .foregroundStyle(Color.themeSecondary)
                Spacer(minLength: 4)
                if let url = page.value?.row.url {
                    ActionButton(icon: .squareArrowOutUpRight, help: "Open in Linear") { Platform.open(url) }
                }
                ActionButton(icon: .rotateCw, help: "Read the issue again", pending: linear.working.contains("read:\(id)")) { asked += 1 }
            }
            if let error = linear.error {
                PanelNote(text: error)
            }
            Group {
                switch page {
                case .loading: PanelLoading()
                case .failed(let message): PanelMessage(text: message, failed: true)
                case .ready(let page): content(page)
                }
            }
            .panelTask(id: Trigger(serverID: target.serverID, asked: asked)) {
                linear.needTeams(of: workspace, on: target.serverID)
                linear.loadIssue(id, of: workspace, on: target.serverID)
            }
        }
    }

    private func content(_ page: LinearPage) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text(page.row.title)
                    .font(.ui(size: 17, weight: .semibold))
                    .foregroundStyle(Color.themeText)
                    .textSelection(.enabled)
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 6) { properties(page) }
                    VStack(alignment: .leading, spacing: 6) { properties(page) }
                }
                if !page.row.labels.isEmpty {
                    HStack(spacing: 6) {
                        ForEach(page.row.labels, id: \.name) { label in
                            Chip(label.name, dot: label.color)
                        }
                    }
                }
                ActionButton("Work on Issue", icon: .play, help: "Start a thread on this issue", variant: .primary) {
                    store.linear.work(on: page, of: workspace, in: target)
                }
                if page.description.isEmpty {
                    Text("No description.")
                        .font(.ui(size: 12.5))
                        .foregroundStyle(Color.themeTertiary)
                } else {
                    PullRequestTextView(blocks: page.description)
                }
                PanelLine()
                VStack(alignment: .leading, spacing: 20) {
                    ForEach(page.comments) { comment in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack(spacing: 6) {
                                LinearInitials(initials: comment.initials, name: comment.author)
                                (Text(comment.author).fontWeight(.semibold).foregroundStyle(Color.themeText)
                                    + Text(" · \(Time.ago(comment.at))").foregroundStyle(Color.themeTertiary))
                                    .font(.ui(size: 12.5))
                            }
                            PullRequestTextView(blocks: comment.body)
                        }
                    }
                    writing
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    @ViewBuilder
    private func properties(_ page: LinearPage) -> some View {
        let linear = store.linear
        let states = linear.teams[workspace]?.first { $0.id == page.row.team }?.states ?? []
        let change = { (change: JSON) in linear.change(id, change, of: workspace, on: target.serverID) }
        let status = AnyView(Image(page.state.symbol, size: ControlSize.small.symbol).foregroundStyle(page.state.color))
        ActionMenu(page.state.name, picture: status, help: "Status", variant: .secondary, size: .small, pending: linear.working.contains(id)) {
            LinearChoices.states(states, current: page.state.id) { change(["state": $0.id]) }
        }
        ActionMenu(
            LinearRow.priorities.first { $0.value == page.row.priority }?.name, icon: page.row.prioritySymbol, help: "Priority",
            variant: .secondary, size: .small
        ) {
            LinearChoices.priorities(current: page.row.priority) { change(["priority": $0]) }
        }
        ActionMenu(page.row.assignee ?? "Unassigned", icon: .circleUser, help: "Assignee", variant: .secondary, size: .small) {
            LinearChoices.assignees(linear.users[workspace] ?? [], current: page.row.assigneeID) { change(["assignee": $0 ?? ""]) }
        }
    }

    private var writing: some View {
        let linear = store.linear
        let words = comment.trimmingCharacters(in: .whitespacesAndNewlines)
        return VStack(alignment: .trailing, spacing: 8) {
            TextArea("Leave a comment", text: $comment, lines: 4)
            ActionButton("Comment", variant: .secondary, pending: linear.working.contains("comment:\(id)")) {
                linear.loadIssue(id, of: workspace, on: target.serverID, comment: words) { comment = "" }
            }
            .disabled(words.isEmpty)
        }
    }
}

/// The sheet that files an issue.
private struct LinearNewIssue: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let target: PanelTarget
    let workspace: String
    /// The team the list shows, when it shows one.
    let team: String?
    @State private var title = ""
    @State private var description = ""
    @State private var teamID: String?
    @State private var stateID: String?
    @State private var assigneeID: String?
    @State private var priority = 0

    var body: some View {
        let linear = store.linear
        let teams = linear.teams[workspace] ?? []
        let picked = teams.first { $0.id == (teamID ?? team) } ?? teams.first
        let states = picked?.states ?? []
        let state = states.first { $0.id == stateID } ?? states.first { $0.kind == "unstarted" } ?? states.first
        let users = linear.users[workspace] ?? []
        VStack(alignment: .leading, spacing: 12) {
            Text("New issue")
                .font(.ui(size: 15, weight: .semibold))
            InputField("Title", text: $title)
            TextArea("Add a description", text: $description, lines: 4, fills: true)
            FlowRow { properties(teams: teams, picked: picked, states: states, state: state, users: users) }
            if let error = linear.error {
                Text(error)
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeDanger)
            }
            HStack(spacing: 8) {
                Spacer()
                ActionButton("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Create Issue", variant: .primary, pending: linear.working.contains("create")) {
                    guard let picked else { return }
                    var issue: JSON = ["team": picked.id, "title": title, "description": description, "priority": priority]
                    if let state { issue["state"] = state.id }
                    if let assigneeID { issue["assignee"] = assigneeID }
                    linear.create(issue, in: workspace, on: target.serverID) { id, identifier in
                        dismiss()
                        store.sidePanel.open(.linearIssue(workspace: workspace, id: id, identifier: identifier))
                    }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(picked == nil || title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        #if os(macOS)
        .frame(width: 520, height: 400)
        #else
        .presentationDetents([.large])
        #endif
        .onAppear { linear.needTeams(of: workspace, on: target.serverID) }
    }

    @ViewBuilder
    private func properties(teams: [LinearTeam], picked: LinearTeam?, states: [LinearState], state: LinearState?, users: [LinearUser]) -> some View {
        ActionMenu(picked?.name ?? "Team", help: "Team", variant: .secondary, size: .small) {
            ForEach(teams) { team in
                Toggle(team.name, isOn: Binding { team.id == picked?.id } set: { _ in
                    teamID = team.id
                    stateID = nil
                })
            }
        }
        ActionMenu(state?.name ?? "Status", icon: state?.symbol, help: "Status", variant: .secondary, size: .small) {
            LinearChoices.states(states, current: state?.id) { stateID = $0.id }
        }
        ActionMenu(
            LinearRow.priorities.first { $0.value == priority }?.name, icon: LinearRow.symbol(priority: priority), help: "Priority",
            variant: .secondary, size: .small
        ) {
            LinearChoices.priorities(current: priority) { priority = $0 }
        }
        ActionMenu(users.first { $0.id == assigneeID }?.name ?? "Unassigned", icon: .circleUser, help: "Assignee", variant: .secondary, size: .small) {
            LinearChoices.assignees(users, current: assigneeID) { assigneeID = $0 }
        }
    }
}
