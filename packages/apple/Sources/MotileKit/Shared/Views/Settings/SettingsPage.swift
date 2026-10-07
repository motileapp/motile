import SwiftUI

/// One section of the settings: its groups of rows, and the way to the group a search picked.
struct SettingsPage: View {
    /// How wide the groups are at most, however wide the window.
    static let contentWidth: CGFloat = 640

    @Environment(AppStore.self) private var store
    let section: SettingsSection
    @AppStorage("appearance") private var appearance = Appearance.system
    @AppStorage(AppStore.steersKey) private var steers = AppStore.steersByDefault
    @State private var setupProject: Project?
    @State private var editedAccount: EditedAccount?

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    switch section {
                    case .general: general
                    case .servers: servers
                    case .agents: agentAccounts
                    case .projects: projects
                    case .textGeneration: textGeneration
                    case .pullRequests: pullRequests
                    }
                }
                .frame(maxWidth: Self.contentWidth)
                .padding(20)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.immediately)
            .onChange(of: store.settingsTarget, initial: true) {
                guard let target = store.settingsTarget else { return }
                withAnimation { proxy.scrollTo(target, anchor: .center) }
                store.settingsTarget = nil
            }
        }
        .onAppear { store.refreshMediaStorage() }
        .sheet(item: $setupProject) { project in
            SetupSheet(project: project)
                .sheetSurface()
        }
        .sheet(item: $editedAccount) { edited in
            AgentAccountSheet(server: edited.server, account: edited.account)
                .sheetSurface()
        }
    }

    @ViewBuilder private var general: some View {
        SettingsGroup("account", "Account") {
            SettingsRow {
                SettingsLabel("Signed in as")
            } trailing: {
                Text(store.account.signedIn ? store.account.email : "Not signed in")
                    .foregroundStyle(Color.themeSecondary)
                if store.account.signedIn {
                    ActionButton("Sign Out", size: .small) { store.signOut() }
                }
            }
        }

        #if os(macOS)
        SettingsGroup("updates", "Updates") {
            SettingsRow {
                SettingsLabel("Motile \(store.updater.current)")
            } trailing: {
                ActionButton("Check for Updates", size: .small, pending: store.updater.state == .checking) { store.updater.check(asked: true) }
            }
            if store.updater.state != .idle {
                ThemeDivider()
                AppUpdateRow(updater: store.updater)
                    .padding(.horizontal, settingsInset)
                    .padding(.vertical, 10)
            }
        }
        #else
        SettingsGroup("updates", "Version") {
            SettingsRow {
                SettingsLabel("Motile \(store.updater.current)")
            } trailing: {
                EmptyView()
            }
        }
        #endif

        SettingsGroup("appearance", "Appearance") {
            SettingsRow {
                SettingsLabel("Theme")
            } trailing: {
                Segmented(Appearance.allCases.map { ($0.label, $0) }, selection: $appearance)
            }
        }

        SettingsGroup("messages", "Messages") {
            SettingsRow {
                SettingsLabel(
                    "Sent while the agent works",
                    description: steers ? "The agent reads it at once, in the turn that runs" : "Waits for the turn to end and starts the next one")
            } trailing: {
                Segmented([("Queue", false), ("Steer", true)], selection: $steers)
            }
        }

        SettingsGroup("storage", "Storage") {
            SettingsRow {
                SettingsLabel("Images and videos", description: storageDescription)
            } trailing: {
                ActionButton("Clear", size: .small) { store.clearMedia() }
                    .disabled((store.mediaStorage?.used ?? 0) == 0)
            }
        }
    }

    /// The servers keep every image and video; the ones kept here only make threads open with them.
    private var storageDescription: String {
        guard let storage = store.mediaStorage else { return "Kept on this \(Platform.device) so threads open with them" }
        let formatter = ByteCountFormatter()
        formatter.countStyle = .file
        formatter.allowsNonnumericFormatting = false
        let (used, limit) = (formatter.string(fromByteCount: storage.used), formatter.string(fromByteCount: storage.limit))
        return "\(used) of \(limit) on this \(Platform.device). Your servers keep them all."
    }

    @ViewBuilder private var servers: some View {
        SettingsGroup("servers", "Servers") {
            ForEach(store.servers) { server in
                SettingsRow {
                    SettingsLabel(server.name, description: description(of: server))
                } trailing: {
                    ServerUpdateStatus(server: server) { EmptyView() }
                    ActionButton("Remove", size: .small) { store.removeServer(server) }
                }
                ThemeDivider()
            }
            SettingsRow {
                ActionButton("Add a Server…", size: .small) { store.showsAddServer = true }
            } trailing: {
                EmptyView()
            }
        }
        continuing
    }

    /// What each server goes on with by itself: threads whose usage limit reset, and threads whose
    /// agents a restart cut off.
    @ViewBuilder private var continuing: some View {
        let servers = store.servers.filter { $0.state == .connected && store.canChooseRestart($0) }
        if !servers.isEmpty {
            SettingsGroup("continue-limits", "Continue after usage limits", caption: "A thread whose agent reached its usage limit goes on once the limit resets") {
                ForEach(servers) { server in
                    SettingsRow {
                        serverName(server)
                    } trailing: {
                        Switch(isOn: Binding { server.continueAfterLimits } set: {
                            store.setContinueSettings(afterLimits: $0, afterRestarts: server.continueAfterRestarts, on: server)
                        })
                    }
                    if server.id != servers.last?.id { ThemeDivider() }
                }
            }
            SettingsGroup("continue-restarts", "Continue after restarts", caption: "When your server comes back from a restart, agents that were working carry on.") {
                ForEach(servers) { server in
                    SettingsRow {
                        serverName(server)
                    } trailing: {
                        Switch(isOn: Binding { server.continueAfterRestarts } set: {
                            store.setContinueSettings(afterLimits: server.continueAfterLimits, afterRestarts: $0, on: server)
                        })
                    }
                    if server.id != servers.last?.id { ThemeDivider() }
                }
            }
        }
    }

    /// The model that writes thread titles, branch names, commit messages and pull requests, and
    /// how it names branches, by server.
    @ViewBuilder private var textGeneration: some View {
        let servers = store.servers.filter { $0.state == .connected && $0.protocolVersion >= 4 }
        if servers.isEmpty {
            SettingsNote("The model that writes thread titles, branch names, commit messages and pull requests is set on each of your servers, once one is connected.")
        } else {
            SettingsGroup("text-model", "Model", caption: "The model that writes thread titles, branch names, commit messages and pull requests") {
                ForEach(servers) { server in
                    SettingsRow {
                        serverName(server)
                    } trailing: {
                        ActionMenu(textModelName(of: server), variant: .secondary, size: .small) {
                            Button("Automatic") { store.setTextModel(nil, on: server) }
                            ForEach(server.models) { model in
                                Button(model.name) { store.setTextModel(model.id, on: server) }
                            }
                        }
                    }
                    if server.id != servers.last?.id { ThemeDivider() }
                }
            }
            let naming = servers.filter { $0.protocolVersion >= 6 }
            if !naming.isEmpty {
                SettingsGroup("branch-names", "Branch names", caption: "How the writer is told to name the branches it makes") {
                    ForEach(naming) { server in
                        if naming.count > 1 {
                            SettingsRow {
                                serverName(server)
                            } trailing: {
                                EmptyView()
                            }
                        }
                        BranchInstructionsEditor(server: server)
                        if server.id != naming.last?.id { ThemeDivider() }
                    }
                }
            }
        }
    }

    /// What each server does with pull requests by itself.
    @ViewBuilder private var pullRequests: some View {
        let servers = store.servers.filter { $0.state == .connected && $0.protocolVersion >= 9 }
        if servers.isEmpty {
            SettingsNote("What your servers do once a thread's pull request merges is set on each of them, once one is connected.")
        } else {
            SettingsGroup("merged", "Mark the thread done after merge or close") {
                ForEach(servers) { server in
                    SettingsRow {
                        serverName(server)
                    } trailing: {
                        Switch(isOn: Binding { server.doneOnMerge } set: {
                            store.setPullRequestSettings(doneOnMerge: $0, removeMergedWorktrees: server.removeMergedWorktrees, on: server)
                        })
                    }
                    if server.id != servers.last?.id { ThemeDivider() }
                }
            }
            SettingsGroup("worktrees", "Remove the thread's worktree after merge") {
                ForEach(servers) { server in
                    SettingsRow {
                        serverName(server)
                    } trailing: {
                        Switch(isOn: Binding { server.removeMergedWorktrees } set: {
                            store.setPullRequestSettings(doneOnMerge: server.doneOnMerge, removeMergedWorktrees: $0, on: server)
                        })
                    }
                    if server.id != servers.last?.id { ThemeDivider() }
                }
            }
        }
    }

    /// The accounts each server's agents work with, a card for each server.
    @ViewBuilder private var agentAccounts: some View {
        if store.servers.isEmpty {
            SettingsNote("The accounts your agents work with are set on each of your servers, once you have one.")
        } else {
            SettingsGroup("agent-accounts", "Accounts", caption: "Each account keeps its sign-in in a folder of its own. A thread works with one and can move to another.", carded: false) {
                ForEach(store.servers) { server in
                    VStack(spacing: 0) {
                        agentAccounts(of: server)
                    }
                    .card()
                }
            }
        }
    }

    @ViewBuilder private func agentAccounts(of server: Server) -> some View {
        SettingsRow {
            HStack(spacing: 8) {
                Image(.server, size: 13)
                Text(server.name)
                    .font(.ui(size: 13, weight: .medium))
            }
            .foregroundStyle(Color.themeText)
        } trailing: {
            if let reason = accountsUnavailable(on: server) {
                Text(reason)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeSecondary)
            }
        }
        if accountsUnavailable(on: server) == nil {
            let installed = Agent.allCases.filter { server.agents[$0] != nil }
            ThemeDivider()
            ForEach(server.agentAccounts.filter { installed.contains($0.agent) }) { account in
                SettingsRow {
                    AgentIcon(agent: account.agent, size: 16)
                    SettingsLabel("\(account.agent.name) · \(account.name)", description: description(of: account))
                } trailing: {
                    ActionButton("Edit…", size: .small) { editedAccount = EditedAccount(server: server, account: account) }
                    if !account.isDefault {
                        ActionButton("Remove", size: .small) { store.removeAgentAccount(account, on: server) }
                    }
                }
                ThemeDivider()
            }
            SettingsRow {
                ActionButton("Add an Account…", size: .small) {
                    editedAccount = EditedAccount(server: server, account: AgentAccount(agent: installed.first ?? .claude))
                }
                .disabled(installed.isEmpty)
            } trailing: {
                EmptyView()
            }
        }
    }

    /// Why a server's accounts can't be shown or changed, if they can't.
    private func accountsUnavailable(on server: Server) -> String? {
        guard server.state == .connected else { return description(of: server) }
        guard server.switchesAccounts else { return "Update it to give its agents more accounts" }
        return nil
    }

    private func description(of account: AgentAccount) -> String {
        let signedIn = account.email.map { [$0, account.plan].compactMap { $0 }.joined(separator: " · ") }
        let who = signedIn ?? (account.variables.isEmpty ? "Not signed in" : "Signs in with its variables")
        return account.folder.isEmpty ? who : "\(who) · \(account.folder)"
    }

    private func serverName(_ server: Server) -> some View {
        SettingsLabel(server.name, icon: .server)
    }

    private func textModelName(of server: Server) -> String {
        server.models.first { $0.id == server.textModel }?.name ?? "Automatic"
    }

    private var projects: some View {
        SettingsGroup("projects", "Projects") {
            ForEach(store.projects) { project in
                SettingsRow {
                    ProjectIcon(project: project, size: 26)
                    SettingsLabel(project.name, description: project.path, truncates: true)
                } trailing: {
                    ActionMenu("Icon", variant: .secondary, size: .small) {
                        Button("Choose an Image…") { store.iconProject = project }
                        Button("Use the Icon in Its Folder") { store.setIcon(of: project, to: nil) }
                    }
                    if (store.server(project.serverID)?.protocolVersion ?? 0) >= 6 {
                        ActionButton("Setup…", help: "The script that runs in each new worktree of \(project.name)", size: .small) {
                            setupProject = project
                        }
                    }
                    ActionButton("Remove", size: .small) { store.removeProject(project) }
                }
                ThemeDivider()
            }
            SettingsRow {
                ActionButton("Add a Project…", size: .small) {
                    store.closeSettings()
                    store.addProject()
                }
                .disabled(store.servers.isEmpty)
            } trailing: {
                EmptyView()
            }
        }
    }

    private func description(of server: Server) -> String {
        let agents = server.agents.sorted { $0.key.rawValue < $1.key.rawValue }.map { "\($0.key.name) \($0.value)" }
        let installed = agents.isEmpty ? "no agent installed" : agents.joined(separator: ", ")
        switch server.state {
        case .connected: return "Connected · version \(server.version) · \(installed)"
        case .connecting: return "Connecting…"
        case .disconnected: return "Offline"
        case .refused: return "This server no longer accepts this \(Platform.device)"
        }
    }
}

private let settingsInset: CGFloat = 14

/// How a server's writer is told to name the branches it makes, to change and to put back.
private struct BranchInstructionsEditor: View {
    @Environment(AppStore.self) private var store
    let server: Server
    @State private var text = ""

    private var changed: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines) != server.branchInstructions }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            TextArea("How to name a branch", text: $text, maxLines: 16)
            HStack(spacing: 8) {
                Spacer()
                ActionButton("Reset", size: .small) {
                    text = server.defaultBranchInstructions
                    store.setBranchInstructions(nil, on: server)
                }
                .disabled(server.branchInstructions == server.defaultBranchInstructions && !changed)
                ActionButton("Save", variant: .primary, size: .small) { store.setBranchInstructions(text, on: server) }
                    .disabled(!changed)
            }
        }
        .padding(.horizontal, settingsInset)
        .padding(.vertical, 10)
        .onAppear { text = server.branchInstructions }
        .onChange(of: server.branchInstructions) { text = server.branchInstructions }
    }
}

/// An account the settings add or change, on its server.
private struct EditedAccount: Identifiable {
    let server: Server
    let account: AgentAccount

    var id: String { "\(server.id)/\(account.id)" }
}

/// An account of an agent to add or change: its name, the folder its sign-in is kept in, and the
/// variables its agent is given, with the command that signs it in.
private struct AgentAccountSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let server: Server
    @State private var account: AgentAccount
    @State private var variables: [EditedVariable]
    @State private var saving = false
    @State private var suggestedFolder = ""

    init(server: Server, account: AgentAccount) {
        self.server = server
        _account = State(initialValue: account)
        _variables = State(initialValue: account.variables.map { EditedVariable(variable: $0) })
    }

    private var installed: [Agent] { Agent.allCases.filter { server.agents[$0] != nil } }
    private var isNew: Bool { account.id.isEmpty }
    private var title: String { isNew ? "New account on \(server.name)" : "\(account.agent.name) account on \(server.name)" }
    private var canSave: Bool { !account.name.trimmingCharacters(in: .whitespaces).isEmpty }

    var body: some View {
        content
            .onChange(of: account.name) { suggestFolder() }
            .onChange(of: account.agent) { suggestFolder() }
    }

    @ViewBuilder private var content: some View {
        #if os(macOS)
        VStack(alignment: .leading, spacing: 14) {
            Text(title)
                .font(.ui(size: 13, weight: .semibold))
            form
            HStack {
                Spacer()
                ActionButton("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, pending: saving) { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canSave)
            }
        }
        .padding(16)
        .frame(width: 460)
        #else
        NavigationStack {
            ScrollView {
                form
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(16)
            }
            .scrollDismissesKeyboard(.interactively)
            .safeAreaInset(edge: .bottom) {
                SheetButton("Save", pending: saving) { save() }
                    .disabled(!canSave)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { SheetCloseButton() }
            }
        }
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled(saving)
        #endif
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: Platform.scale > 1 ? 28 : 14) {
            if isNew && installed.count > 1 {
                Segmented(installed.map { ($0.name, $0) }, selection: $account.agent)
            }
            field("Name") {
                InputField("Personal", text: $account.name)
            }
            if !account.isDefault {
                field("Folder", caption: "Where its sign-in is kept, as \(account.folderVariable)") {
                    InputField(account.agent == .claude ? "~/.claude-personal" : "~/.codex-personal", text: $account.folder, monospaced: true)
                }
                if account.agent == .codex {
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Share sessions with the default account")
                                .font(.ui(size: 13, weight: .medium))
                            Text("The folder keeps only the sign-in. Threads move between the two and go on where they were.")
                                .font(.ui(size: 11.5))
                                .foregroundStyle(Color.themeSecondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        .padding(.horizontal, 4)
                        Spacer(minLength: 12)
                        Switch(isOn: $account.sharesSessions)
                    }
                }
            }
            field("Variables", caption: "Given to its agent: an API key or a router. A sensitive value stays on \(server.name) and is never shown again.") {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach($variables) { $edited in
                        variableRow($edited)
                    }
                    ActionButton("Add a Variable", icon: .plus, size: .small) {
                        variables.append(EditedVariable(variable: AgentAccount.Variable(name: "", value: "", sensitive: true)))
                    }
                }
            }
            signIn
        }
    }

    /// How the account is signed in: on its server, with its folder.
    private var signIn: some View {
        field("Sign In", caption: "Run it in a terminal on \(server.name). Who is signed in shows in the list once the agent says.") {
            CommandBox(command: account.signInCommand)
        }
    }

    /// A variable's name and value, with whether the value is sensitive and the way to take it out.
    private func variableRow(_ edited: Binding<EditedVariable>) -> some View {
        let variable = edited.wrappedValue.variable
        let kept = variable.sensitive && account.variables.contains { $0.name == variable.name && $0.sensitive }
        return HStack(spacing: 6) {
            InputField("NAME", text: edited.variable.name, size: .small, monospaced: true)
                .frame(width: 150)
            InputField(kept ? "••••••••" : "value", text: edited.variable.value, size: .small, monospaced: true, secure: variable.sensitive)
            ActionButton(
                icon: variable.sensitive ? .eyeOff : .eye, help: variable.sensitive ? "Sensitive: hidden and kept on \(server.name)" : "Shown: mark it sensitive",
                size: .small, selected: variable.sensitive
            ) {
                edited.wrappedValue.variable.sensitive.toggle()
            }
            ActionButton(icon: .x, help: "Remove the variable", size: .small) {
                variables.removeAll { $0.id == edited.wrappedValue.id }
            }
        }
    }

    private func field<Content: View>(_ title: String, caption: String? = nil, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.ui(size: 12, weight: .medium))
                .padding(.horizontal, 4)
            content()
            if let caption {
                Text(caption)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(Color.themeTertiary)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 4)
            }
        }
    }

    /// Fills a new account's folder from its name until the folder is typed by hand.
    private func suggestFolder() {
        guard isNew, account.folder.isEmpty || account.folder == suggestedFolder else { return }
        suggestedFolder = account.suggestedFolder(besides: server.agentAccounts)
        account.folder = suggestedFolder
    }

    private func save() {
        var saved = account
        saved.variables = variables.map(\.variable).filter { !$0.name.trimmingCharacters(in: .whitespaces).isEmpty }
        if saved.agent != .codex { saved.sharesSessions = false }
        saving = true
        store.saveAgentAccount(saved, on: server) { kept in
            saving = false
            if kept { dismiss() }
        }
    }
}

/// A variable as the account's sheet edits it.
private struct EditedVariable: Identifiable {
    let id = UUID()
    var variable: AgentAccount.Variable
}

/// The shell script that runs in each new worktree of a project before the agent starts there.
private struct SetupSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let project: Project
    @State private var script = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Worktree setup for \(project.name)")
                .font(.ui(size: 13, weight: .semibold))
            Text("A shell script that runs in each new worktree before the agent starts there, to install what the work needs. $MOTILE_PROJECT is the project's folder, as in: cp \"$MOTILE_PROJECT/.env\" . && pnpm install")
                .font(.caption)
                .foregroundStyle(Color.themeSecondary)
                .fixedSize(horizontal: false, vertical: true)
            TextArea("cp \"$MOTILE_PROJECT/.env\" . && pnpm install", text: $script, monospaced: true, lines: 5, maxLines: 14)
            HStack {
                Spacer()
                ActionButton("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary) {
                    store.setSetup(of: project, to: script)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(16)
        #if os(macOS)
        .frame(width: 440)
        #else
        .frame(maxHeight: .infinity, alignment: .top)
        .presentationDetents([.medium, .large])
        #endif
        .onAppear { script = project.setup ?? "" }
    }
}

private struct SettingsGroup<Content: View>: View {
    private let id: String
    private let title: String
    private let caption: String?
    /// Whether its rows share one card, or bring cards of their own.
    private let carded: Bool
    private let content: Content

    init(_ id: String, _ title: String, caption: String? = nil, carded: Bool = true, @ViewBuilder content: () -> Content) {
        self.id = id
        self.title = title
        self.caption = caption
        self.carded = carded
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeText)
                if let caption {
                    Text(caption)
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeTertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.horizontal, 4)
            if carded {
                VStack(spacing: 0) {
                    content
                }
                .card()
            } else {
                VStack(spacing: 12) {
                    content
                }
            }
        }
        .id(id)
    }
}

/// What a row is about: its name, and under it what it does.
private struct SettingsLabel: View {
    let title: String
    var description: String?
    var icon: Symbol?
    /// A description that is a path is cut at its start, not wrapped.
    var truncates = false

    init(_ title: String, description: String? = nil, icon: Symbol? = nil, truncates: Bool = false) {
        self.title = title
        self.description = description
        self.icon = icon
        self.truncates = truncates
    }

    var body: some View {
        HStack(spacing: 8) {
            if let icon {
                Image(icon, size: 13)
                    .foregroundStyle(Color.themeSecondary)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.ui(size: 13, weight: .medium))
                if let description {
                    Text(description)
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeSecondary)
                        .lineLimit(truncates ? 1 : nil)
                        .truncationMode(.head)
                        .fixedSize(horizontal: false, vertical: !truncates)
                }
            }
        }
    }
}

/// What the row is about on the left, its controls on the right, with the same room above and
/// below.
private struct SettingsRow<Leading: View, Trailing: View>: View {
    @ViewBuilder let leading: Leading
    @ViewBuilder let trailing: Trailing

    var body: some View {
        // A row too narrow for both puts its controls under what they are about.
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 10) {
                leading
                Spacer(minLength: 12)
                trailing
            }
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 10) { leading }
                HStack(spacing: 10) { trailing }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, settingsInset)
        .padding(.vertical, 10)
        .frame(minHeight: 46)
    }
}

/// Said on a page with nothing to set yet.
private struct SettingsNote: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.ui(size: 13))
            .foregroundStyle(Color.themeSecondary)
            .fixedSize(horizontal: false, vertical: true)
            .padding(settingsInset)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card()
    }
}
