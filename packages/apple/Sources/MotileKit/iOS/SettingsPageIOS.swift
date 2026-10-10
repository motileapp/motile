#if os(iOS)
import SwiftUI

/// The settings' sections in groups, each pushing its page, or selected beside it on a wide
/// sheet. While a search is typed, the groups it finds take their place.
struct SettingsSections: View {
    private static let groups: [[SettingsSection]] = [[.general], [.servers, .agents, .projects], [.textGeneration, .pullRequests]]

    @Environment(AppStore.self) private var store
    @Environment(\.surface) private var surface
    /// The section shown beside the list, on a wide sheet, lit in it. Without one, each pushes.
    var selected: SettingsSection?
    let choose: (SettingsSection) -> Void

    var body: some View {
        @Bindable var store = store
        SettingsList { sections }
            .searchable(text: $store.settingsQuery, prompt: "Search")
    }

    @ViewBuilder private var sections: some View {
        if store.settingsQuery.isEmpty {
            ForEach(Self.groups, id: \.self) { group in
                Section {
                    ForEach(group) { section in
                        if let selected {
                            Button { choose(section) } label: { row(section) }
                                .foregroundStyle(Color.themeForeground)
                                .listRowBackground(surface.color(selected == section ? .rowSelected : .box))
                        } else {
                            NavigationLink(value: section) { row(section) }
                        }
                    }
                }
            }
        } else {
            results
        }
    }

    private func row(_ section: SettingsSection) -> some View {
        Label {
            Text(section.title)
        } icon: {
            SettingsTile(symbol: section.symbol)
        }
    }

    @ViewBuilder private var results: some View {
        let found = SettingsEntry.matching(store.settingsQuery)
        if found.isEmpty {
            ContentUnavailableView.search(text: store.settingsQuery)
                .listRowBackground(Color.clear)
        } else {
            Section {
                ForEach(found) { entry in
                    Button {
                        store.openSettings(entry.section, target: entry.id)
                        choose(entry.section)
                    } label: {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(entry.title)
                                    .foregroundStyle(Color.themeForeground)
                                Text(entry.section.title)
                                    .font(.footnote)
                                    .foregroundStyle(Color.themeMutedForeground)
                            }
                        } icon: {
                            SettingsTile(symbol: entry.section.symbol)
                        }
                    }
                }
            }
        }
    }
}

/// One section of the settings as a list of the system's groups, scrolled to the group a search
/// picked.
struct SettingsPage: View {
    @Environment(AppStore.self) private var store
    let section: SettingsSection
    @AppStorage("appearance") private var appearance = Appearance.system
    @AppStorage(AppStore.steersKey) private var steers = AppStore.steersByDefault
    @State private var editedAccount: EditedAccount?
    @State private var removal: Removal?

    var body: some View {
        ScrollViewReader { proxy in
            SettingsList {
                switch section {
                case .general: general
                case .servers: servers
                case .agents: agentAccounts
                case .projects: projects
                case .textGeneration: textGeneration
                case .pullRequests: pullRequests
                }
            }
            .onChange(of: store.settingsTarget, initial: true) {
                guard let target = store.settingsTarget else { return }
                withAnimation { proxy.scrollTo(target, anchor: .center) }
                store.settingsTarget = nil
            }
        }
        .onAppear { store.refreshMediaStorage() }
        .confirmsRemoval($removal)
        .sheet(item: $editedAccount) { edited in
            AgentAccountSheet(server: edited.server, account: edited.account)
                .sheetSurface()
        }
    }

    @ViewBuilder private var general: some View {
        Section("Account") {
            LabeledContent("Signed in as", value: store.account.signedIn ? store.account.email : "Not signed in")
                .lineLimit(1)
                .truncationMode(.middle)
            if store.account.signedIn {
                Button("Sign Out", role: .destructive) { store.signOut() }
                    .foregroundStyle(Color.themeDestructive)
            }
        }
        .id("account")

        Section("Appearance") {
            Picker("Theme", selection: $appearance) {
                ForEach(Appearance.allCases) { Text($0.label).tag($0) }
            }
            .tint(Color.themeForeground)
        }
        .id("appearance")

        Section {
            Picker("Sent while the agent works", selection: $steers) {
                Text("Queue").tag(false)
                Text("Steer").tag(true)
            }
            .tint(Color.themeForeground)
        } header: {
            Text("Messages")
        } footer: {
            Text(AppStore.steersDescription(steers))
        }
        .id("messages")

        Section {
            LabeledContent("Images and videos", value: store.mediaStorageUsed)
            Button("Clear Images and Videos", role: .destructive) { store.clearMedia() }
                .foregroundStyle(Color.themeDestructive)
                .disabled((store.mediaStorage?.used ?? 0) == 0)
        } header: {
            Text("Storage")
        } footer: {
            Text("Kept on this \(Platform.device) so threads open with them. Your servers keep them all.")
        }
        .id("storage")

        Section("About") {
            LabeledContent("Version", value: store.updater.current)
        }
        .id("updates")
    }

    @ViewBuilder private var servers: some View {
        Section {
            ForEach(store.servers) { server in
                NavigationLink {
                    ServerSettingsPage(serverID: server.id)
                } label: {
                    SettingsItem(server.name, description: server.state == .connected ? "Connected · version \(server.version)" : server.stateLabel) {
                        SettingsTile(symbol: .server)
                    }
                }
            }
            NavigationLink {
                AddServerPage()
            } label: {
                SettingsActionLabel("Add a Server", symbol: .plus)
            }
        }
        .id("servers")
        let continuing = store.servers.filter { $0.state == .connected && store.canChooseRestart($0) }
        if !continuing.isEmpty {
            SettingsToggles(
                "Continue after usage limits", servers: continuing,
                caption: "A thread whose agent reached its usage limit goes on once the limit resets."
            ) { server in
                Binding { server.continueAfterLimits } set: {
                    store.setContinueSettings(afterLimits: $0, afterRestarts: server.continueAfterRestarts, on: server)
                }
            }
            .id("continue-limits")
            SettingsToggles(
                "Continue after restarts", servers: continuing,
                caption: "When your server comes back from a restart, agents that were working carry on."
            ) { server in
                Binding { server.continueAfterRestarts } set: {
                    store.setContinueSettings(afterLimits: server.continueAfterLimits, afterRestarts: $0, on: server)
                }
            }
            .id("continue-restarts")
        }
    }

    @ViewBuilder private var agentAccounts: some View {
        if store.servers.isEmpty {
            SettingsNote("The accounts your agents work with are set on each of your servers, once you have one.")
        } else {
            ForEach(store.servers) { server in
                Section {
                    agentAccounts(of: server)
                } header: {
                    Label(server.name, symbol: .server, size: 12)
                } footer: {
                    if server.id == store.servers.last?.id {
                        Text("Each account keeps its sign-in in a folder of its own. A thread works with one and can move to another.")
                    }
                }
            }
            .id("agent-accounts")
        }
    }

    @ViewBuilder private func agentAccounts(of server: Server) -> some View {
        if let reason = server.accountsUnavailable {
            Text(reason)
                .foregroundStyle(Color.themeMutedForeground)
        } else {
            let installed = Agent.allCases.filter { server.agents[$0] != nil }
            ForEach(server.agentAccounts.filter { installed.contains($0.agent) }) { account in
                Button {
                    editedAccount = EditedAccount(server: server, account: account)
                } label: {
                    SettingsItem("\(account.agent.name) · \(account.name)", description: account.settingsDescription) {
                        AgentIcon(agent: account.agent, size: 22)
                            .frame(width: SettingsTile.size, height: SettingsTile.size)
                    }
                }
                .swipeActions {
                    if !account.isDefault {
                        Button("Remove", role: .destructive) { removal = .account(account, on: server) }
                            .tint(Color.themeDestructive)
                    }
                }
                .contextMenu {
                    Button { editedAccount = EditedAccount(server: server, account: account) } label: { Label("Edit", symbol: .pencil) }
                    if !account.isDefault {
                        Button(role: .destructive) { removal = .account(account, on: server) } label: { Label("Remove Account", symbol: .trash2) }
                    }
                }
            }
            Button {
                editedAccount = EditedAccount(server: server, account: AgentAccount(agent: installed.first ?? .claude))
            } label: {
                SettingsActionLabel("Add an Account", symbol: .plus)
            }
            .disabled(installed.isEmpty)
        }
    }

    private var projects: some View {
        Section {
            ForEach(store.projects) { project in
                NavigationLink {
                    ProjectSettingsPage(projectID: project.id)
                } label: {
                    SettingsItem(project.name, description: project.path, truncates: .head) {
                        ProjectIcon(project: project, size: SettingsTile.size)
                    }
                }
            }
            NavigationLink {
                CommandPanel(start: store.addProjectPage, inStack: true)
            } label: {
                SettingsActionLabel("Add a Project", symbol: .plus)
            }
            .disabled(store.servers.isEmpty)
        }
        .id("projects")
    }

    @ViewBuilder private var textGeneration: some View {
        let servers = store.servers.filter { $0.state == .connected && $0.protocolVersion >= 4 }
        if servers.isEmpty {
            SettingsNote("The model that writes thread titles, branch names, commit messages and pull requests is set on each of your servers, once one is connected.")
        } else {
            Section {
                ForEach(servers) { server in
                    Picker(selection: Binding { server.textModel } set: { store.setTextModel($0, on: server) }) {
                        Text("Automatic").tag(String?.none)
                        ForEach(server.distinctModels) { model in
                            Text(model.name).tag(String?.some(model.id))
                        }
                    } label: {
                        Text(servers.count > 1 ? server.name : "Model")
                    }
                    .tint(Color.themeForeground)
                }
            } header: {
                if servers.count > 1 { Text("Model") }
            } footer: {
                Text("The model that writes thread titles, branch names, commit messages and pull requests.")
            }
            .id("text-model")
            let naming = servers.filter { $0.protocolVersion >= 6 }
            if !naming.isEmpty {
                Section {
                    ForEach(naming) { server in
                        NavigationLink {
                            BranchNamesPage(serverID: server.id)
                        } label: {
                            LabeledContent(
                                naming.count > 1 ? server.name : "Branch Names",
                                value: server.branchInstructions == server.defaultBranchInstructions ? "Default" : "Custom")
                        }
                    }
                } header: {
                    if naming.count > 1 { Text("Branch names") }
                } footer: {
                    Text("How the writer is told to name the branches it makes.")
                }
                .id("branch-names")
            }
        }
    }

    @ViewBuilder private var pullRequests: some View {
        let servers = store.servers.filter { $0.state == .connected && $0.protocolVersion >= 9 }
        if servers.isEmpty {
            SettingsNote("What your servers do once a thread's pull request merges is set on each of them, once one is connected.")
        } else {
            let doneOnMerge = { (server: Server) in
                Binding { server.doneOnMerge } set: {
                    store.setPullRequestSettings(doneOnMerge: $0, removeMergedWorktrees: server.removeMergedWorktrees, on: server)
                }
            }
            let removesWorktrees = { (server: Server) in
                Binding { server.removeMergedWorktrees } set: {
                    store.setPullRequestSettings(doneOnMerge: server.doneOnMerge, removeMergedWorktrees: $0, on: server)
                }
            }
            if servers.count == 1, let server = servers.first {
                Section {
                    Toggle("Mark the thread done after merge or close", isOn: doneOnMerge(server))
                        .id("merged")
                    Toggle("Remove the thread's worktree after merge", isOn: removesWorktrees(server))
                        .id("worktrees")
                }
            } else {
                SettingsToggles("Mark the thread done after merge or close", servers: servers, isOn: doneOnMerge)
                    .id("merged")
                SettingsToggles("Remove the thread's worktree after merge", servers: servers, isOn: removesWorktrees)
                    .id("worktrees")
            }
        }
    }
}

/// A server's page: how it is, its agents, its update, and the way to remove it.
private struct ServerSettingsPage: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let serverID: String
    @State private var removal: Removal?

    var body: some View {
        SettingsList {
            if let server = store.server(serverID) {
                Section {
                    LabeledContent("Status", value: server.stateLabel)
                    if server.state == .connected {
                        LabeledContent("Version", value: server.version)
                        ForEach(Agent.allCases.filter { server.agents[$0] != nil }, id: \.self) { agent in
                            LabeledContent(agent.name, value: server.agents[agent] ?? "")
                        }
                    }
                }
                if store.isOutdated(server) || store.serverUpdate(of: server) != nil {
                    Section {
                        LabeledContent(store.serverUpdate(of: server) == nil ? "Update available" : "Updating") {
                            ServerUpdateStatus(server: server, variant: .secondary, size: .regular) { EmptyView() }
                        }
                    }
                }
                Section {
                    Button("Remove Server", role: .destructive) { removal = .server(server) }
                        .foregroundStyle(Color.themeDestructive)
                }
            }
        }
        .navigationTitle(store.server(serverID)?.name ?? "Server")
        .navigationBarTitleDisplayMode(.inline)
        .confirmsRemoval($removal)
        .onChange(of: store.server(serverID) == nil) { _, gone in
            guard gone else { return }
            dismiss()
        }
    }
}

/// The command that links a new server, until that server shows up.
private struct AddServerPage: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ConnectServerView(isFirst: false)
            .navigationTitle("Add a Server")
            .navigationBarTitleDisplayMode(.inline)
            .onChange(of: store.servers.count) { old, new in
                guard new > old else { return }
                dismiss()
            }
    }
}

/// A project's page: where it is, its icon, the script its new worktrees run, and the way to
/// remove it.
private struct ProjectSettingsPage: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let projectID: String
    @State private var removal: Removal?

    var body: some View {
        SettingsList {
            if let project = store.projects.first(where: { $0.id == projectID }) {
                Section {
                    VStack(spacing: 4) {
                        ProjectIcon(project: project, size: 56)
                            .padding(.bottom, 8)
                        Text(project.name)
                            .font(.title3.weight(.semibold))
                            .foregroundStyle(Color.themeForeground)
                        Text(project.path)
                            .font(.footnote)
                            .foregroundStyle(Color.themeMutedForeground)
                            .lineLimit(2)
                            .truncationMode(.head)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
                }
                Section {
                    if let server = store.server(project.serverID) {
                        LabeledContent("Server", value: server.name)
                    }
                }
                Section {
                    NavigationLink {
                        CommandPanel(start: .icon(project.id), inStack: true)
                    } label: {
                        SettingsActionLabel("Choose an Icon", symbol: .image)
                    }
                    if (store.server(project.serverID)?.protocolVersion ?? 0) >= 6 {
                        NavigationLink {
                            WorktreeSetupPage(project: project)
                        } label: {
                            LabeledContent("Worktree Setup", value: project.setup == nil ? "None" : "Script")
                        }
                    }
                }
                Section {
                    Button("Remove Project", role: .destructive) { removal = .project(project) }
                        .foregroundStyle(Color.themeDestructive)
                }
            }
        }
        .navigationTitle(store.projects.first { $0.id == projectID }?.name ?? "Project")
        .navigationBarTitleDisplayMode(.inline)
        .confirmsRemoval($removal)
        .onChange(of: store.projects.contains { $0.id == projectID }) { _, kept in
            guard !kept else { return }
            dismiss()
        }
    }
}

/// The shell script that runs in each new worktree of a project before the agent starts there.
private struct WorktreeSetupPage: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let project: Project
    @State private var script = ""

    var body: some View {
        SettingsList {
            Section {
                SettingsTextEditor(placeholder: "cp \"$MOTILE_PROJECT/.env\" . && pnpm install", text: $script, monospaced: true)
            } footer: {
                Text("Runs in each new worktree before the agent starts there, to install what the work needs. $MOTILE_PROJECT is the project's folder.")
            }
        }
        .navigationTitle("Worktree Setup")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    store.setSetup(of: project, to: script)
                    dismiss()
                }
                .disabled(script == (project.setup ?? ""))
            }
        }
        .onAppear { script = project.setup ?? "" }
    }
}

/// How a server's writer is told to name the branches it makes, to change and to put back.
private struct BranchNamesPage: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let serverID: String
    @State private var text = ""

    var body: some View {
        SettingsList {
            if let server = store.server(serverID) {
                Section {
                    SettingsTextEditor(placeholder: "How to name a branch", text: $text)
                } footer: {
                    Text("How the writer on \(server.name) is told to name the branches it makes.")
                }
                Section {
                    Button("Reset to Default") {
                        text = server.defaultBranchInstructions
                        store.setBranchInstructions(nil, on: server)
                    }
                    .disabled(server.branchInstructions == server.defaultBranchInstructions && text == server.branchInstructions)
                }
            }
        }
        .navigationTitle("Branch Names")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    guard let server = store.server(serverID) else { return }
                    store.setBranchInstructions(text, on: server)
                    dismiss()
                }
                .disabled(!changed)
            }
        }
        .onAppear { text = store.server(serverID)?.branchInstructions ?? "" }
    }

    private var changed: Bool {
        text.trimmingCharacters(in: .whitespacesAndNewlines) != store.server(serverID)?.branchInstructions
    }
}

/// The settings' list: the system's inset groups, drawn on the theme's colours.
struct SettingsList<Content: View>: View {
    @Environment(\.surface) private var surface
    private let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        List {
            content
                .listRowBackground(surface.color(.box))
                .listRowSeparatorTint(Color.themeBorder)
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .background(Color.themeBackground.ignoresSafeArea())
        .tint(Color.themePrimary)
    }
}

/// A row's picture: its symbol, as wide as the agents' and projects' icons beside it.
struct SettingsTile: View {
    static let size: CGFloat = 30
    let symbol: Symbol

    var body: some View {
        Image(symbol, size: 16)
            .foregroundStyle(Color.themeForeground)
            .frame(width: Self.size, height: Self.size)
    }
}

/// A button's row in a list: its symbol and its words, in the tint.
struct SettingsActionLabel: View {
    let title: String
    let symbol: Symbol

    init(_ title: String, symbol: Symbol) {
        self.title = title
        self.symbol = symbol
    }

    var body: some View {
        Label {
            Text(title)
        } icon: {
            Image(symbol, size: 17)
        }
        .foregroundStyle(Color.themePrimary)
    }
}

/// A row that names something, with what it is under it and its picture before it.
private struct SettingsItem<Picture: View>: View {
    let title: String
    let description: String
    var truncates: Text.TruncationMode = .tail
    @ViewBuilder let picture: Picture

    init(_ title: String, description: String, truncates: Text.TruncationMode = .tail, @ViewBuilder picture: () -> Picture) {
        self.title = title
        self.description = description
        self.truncates = truncates
        self.picture = picture()
    }

    var body: some View {
        HStack(spacing: 12) {
            picture
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .foregroundStyle(Color.themeForeground)
                Text(description)
                    .font(.footnote)
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
                    .truncationMode(truncates)
            }
        }
        .padding(.vertical, 2)
    }
}

/// A setting each server has: a switch alone when there is one server, a switch for each under
/// the setting's name when there are more.
private struct SettingsToggles: View {
    let title: String
    let servers: [Server]
    var caption: String?
    let isOn: (Server) -> Binding<Bool>

    init(_ title: String, servers: [Server], caption: String? = nil, isOn: @escaping (Server) -> Binding<Bool>) {
        self.title = title
        self.servers = servers
        self.caption = caption
        self.isOn = isOn
    }

    var body: some View {
        Section {
            if servers.count == 1, let server = servers.first {
                Toggle(title, isOn: isOn(server))
            } else {
                ForEach(servers) { server in
                    Toggle(server.name, isOn: isOn(server))
                }
            }
        } header: {
            if servers.count > 1 { Text(title) }
        } footer: {
            if let caption { Text(caption) }
        }
    }
}

/// Text to write a few lines of, in a row of its own.
private struct SettingsTextEditor: View {
    let placeholder: String
    @Binding var text: String
    var monospaced = false

    var body: some View {
        TextField(placeholder, text: $text, axis: .vertical)
            .font(monospaced ? .system(.callout, design: .monospaced) : .body)
            .lineLimit(6...16)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .padding(.vertical, 4)
    }
}

/// Said on a page with nothing to set yet.
private struct SettingsNote: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Section {
            Text(text)
                .foregroundStyle(Color.themeMutedForeground)
        }
    }
}
#endif
