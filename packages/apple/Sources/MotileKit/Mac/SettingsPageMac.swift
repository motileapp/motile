#if os(macOS)
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
    @State private var removal: Removal?

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
                .padding(.horizontal, 20)
                .padding(.top, 20)
                .padding(.bottom, 80)
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
        .confirmsRemoval($removal)
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
                SettingsLabel("Signed in as", description: store.account.signedIn ? store.account.email : "Not signed in", truncates: .middle)
            } trailing: {
                if store.account.signedIn {
                    ActionButton("Sign Out", variant: .outline, size: .large) { store.signOut() }
                }
            }
        }

        SettingsGroup("updates", "Updates") {
            SettingsRow {
                SettingsLabel("Motile \(store.updater.current)")
            } trailing: {
                ActionButton("Check for Updates", variant: .outline, size: .large, pending: store.updater.state == .checking) { store.updater.check(asked: true) }
            }
            if store.updater.state != .idle {
                ThemeDivider()
                AppUpdateRow(updater: store.updater, variant: .outline, size: .large)
                    .padding(.leading, settingsInset)
                    .padding(.trailing, settingsInset - rowOutset(for: ControlSize.large.height))
                    .padding(.vertical, (settingsRowHeight - ControlSize.large.height) / 2)
            }
        }

        SettingsGroup("appearance", "Appearance") {
            SettingsRow {
                SettingsLabel("Theme")
            } trailing: {
                Segmented(Appearance.allCases.map { ($0.label, $0) }, selection: $appearance)
                    .padding(.trailing, -segmentedOutset)
            }
        }

        SettingsGroup("messages", "Messages") {
            SettingsRow {
                SettingsLabel(
                    "Sent while the agent works",
                    description: AppStore.steersDescription(steers))
            } trailing: {
                Segmented([("Queue", false), ("Steer", true)], selection: $steers)
                    .padding(.trailing, -segmentedOutset)
            }
        }

        SettingsGroup("storage", "Storage") {
            SettingsRow {
                SettingsLabel("Images and videos", description: store.mediaStorageDescription)
            } trailing: {
                ActionButton("Clear", variant: .outline, size: .large) { store.clearMedia() }
                    .disabled((store.mediaStorage?.used ?? 0) == 0)
            }
        }
    }

    @ViewBuilder private var servers: some View {
        SettingsGroup("servers", "Servers") {
            ForEach(store.servers) { server in
                SettingsRow {
                    SettingsLabel(server.name, description: server.settingsDescription, truncates: .tail)
                } trailing: {
                    ServerUpdateStatus(server: server, variant: .outline, size: .large) { EmptyView() }
                    more("What to do with \(server.name)") {
                        item("Remove Server…", symbol: .trash2, role: .destructive) { removal = .server(server) }
                    }
                }
                ThemeDivider()
            }
            SettingsRow {
                ActionButton("Add a Server", icon: .plus, variant: .outline, size: .large) { store.showsAddServer = true }
                    .padding(.leading, -rowOutset(for: ControlSize.large.height))
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
                        ActionMenu(server.textModelName, variant: .outline, size: .large) {
                            Button("Automatic") { store.setTextModel(nil, on: server) }
                            ForEach(server.distinctModels) { model in
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
            .foregroundStyle(Color.themeForeground)
        } trailing: {
            if let reason = server.accountsUnavailable {
                Text(reason)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        if server.accountsUnavailable == nil {
            let installed = Agent.allCases.filter { server.agents[$0] != nil }
            ThemeDivider()
            ForEach(server.agentAccounts.filter { installed.contains($0.agent) }) { account in
                SettingsRow {
                    AgentIcon(agent: account.agent, size: 16)
                    SettingsLabel("\(account.agent.name) · \(account.name)", description: account.settingsDescription)
                } trailing: {
                    more("What to do with the account") {
                        item("Edit…", symbol: .pencil) { editedAccount = EditedAccount(server: server, account: account) }
                        if !account.isDefault {
                            Divider()
                            item("Remove Account…", symbol: .trash2, role: .destructive) { removal = .account(account, on: server) }
                        }
                    }
                }
                ThemeDivider()
            }
            SettingsRow {
                ActionButton("Add an Account", icon: .plus, variant: .outline, size: .large) {
                    editedAccount = EditedAccount(server: server, account: AgentAccount(agent: installed.first ?? .claude))
                }
                .padding(.leading, -rowOutset(for: ControlSize.large.height))
                .disabled(installed.isEmpty)
            } trailing: {
                EmptyView()
            }
        }
    }

    private func serverName(_ server: Server) -> some View {
        SettingsLabel(server.name, icon: .server)
    }

    private var projects: some View {
        SettingsGroup("projects", "Projects") {
            ForEach(store.projects) { project in
                SettingsRow {
                    ProjectIcon(project: project, size: 26)
                    SettingsLabel(project.name, description: project.path, truncates: .head)
                } trailing: {
                    more("What to do with \(project.name)") {
                        item("Choose an Icon…", symbol: .image) { store.openPanel(.icon(project.id)) }
                        if (store.server(project.serverID)?.protocolVersion ?? 0) >= 6 {
                            item("Worktree Setup…", symbol: .terminal) { setupProject = project }
                        }
                        Divider()
                        item("Remove Project…", symbol: .trash2, role: .destructive) { removal = .project(project) }
                    }
                }
                ThemeDivider()
            }
            SettingsRow {
                ActionButton("Add a Project", icon: .plus, variant: .outline, size: .large) {
                    store.closeSettings()
                    store.addProject()
                }
                .padding(.leading, -rowOutset(for: ControlSize.large.height))
                .disabled(store.servers.isEmpty)
            } trailing: {
                EmptyView()
            }
        }
    }

    /// The row's other actions, under its three dots.
    private func more<Items: View>(_ help: String, @ViewBuilder items: () -> Items) -> some View {
        ActionMenu(icon: .ellipsis, help: help, size: .large, content: items)
    }

    private func item(_ title: String, symbol: Symbol, role: ButtonRole? = nil, action: @escaping () -> Void) -> some View {
        Button(role: role, action: action) {
            Label {
                Text(title)
            } icon: {
                Image(platform: .symbol(symbol, size: 13))
            }
        }
    }
}

private let settingsInset: CGFloat = 14
private let settingsRowHeight: CGFloat = scaled(48)

/// What stands a control this tall as far from its row's right as from its top and bottom.
private func rowOutset(for height: CGFloat) -> CGFloat {
    settingsInset - (settingsRowHeight - height) / 2
}

/// A row's controls are large; a segmented picker is a little taller than one, so it reaches a
/// little further.
private var segmentedOutset: CGFloat {
    let track = Segmented<Bool>.inset + Segmented<Bool>.border
    return rowOutset(for: ControlSize.regular.height + 2 * track) - rowOutset(for: ControlSize.large.height)
}

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
                ActionButton("Reset", variant: .outline, size: .large) {
                    text = server.defaultBranchInstructions
                    store.setBranchInstructions(nil, on: server)
                }
                .disabled(server.branchInstructions == server.defaultBranchInstructions && !changed)
                ActionButton("Save", variant: .primary, size: .large) { store.setBranchInstructions(text, on: server) }
                    .disabled(!changed)
            }
        }
        .padding(settingsInset)
        .onAppear { text = server.branchInstructions }
        .onChange(of: server.branchInstructions) { text = server.branchInstructions }
    }
}

/// The shell script that runs in each new worktree of a project before the agent starts there.
private struct SetupSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let project: Project
    @State private var script = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Worktree Setup for \(project.name)")
                .font(.ui(size: 13, weight: .semibold))
            Text("A shell script that runs in each new worktree before the agent starts there, to install what the work needs. $MOTILE_PROJECT is the project's folder, as in: cp \"$MOTILE_PROJECT/.env\" . && pnpm install")
                .font(.caption)
                .foregroundStyle(Color.themeMutedForeground)
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
        .frame(width: 440)
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
                    .foregroundStyle(Color.themeForeground)
                if let caption {
                    Text(caption)
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
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
    /// A description that is cut on one line instead of wrapped: a path at its start, a sentence at its end.
    var truncates: Text.TruncationMode?

    init(_ title: String, description: String? = nil, icon: Symbol? = nil, truncates: Text.TruncationMode? = nil) {
        self.title = title
        self.description = description
        self.icon = icon
        self.truncates = truncates
    }

    var body: some View {
        HStack(spacing: 8) {
            if let icon {
                Image(icon, size: 13)
                    .foregroundStyle(Color.themeMutedForeground)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.ui(size: 13, weight: .medium))
                if let description {
                    Text(description)
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeMutedForeground)
                        .lineLimit(truncates == nil ? nil : 1)
                        .truncationMode(truncates ?? .tail)
                        .fixedSize(horizontal: false, vertical: truncates == nil)
                }
            }
        }
    }
}

/// What the row is about on the left, its controls on the right. Every row is as tall as the
/// tallest, with the same room above and below.
private struct SettingsRow<Leading: View, Trailing: View>: View {
    @ViewBuilder let leading: Leading
    @ViewBuilder let trailing: Trailing

    var body: some View {
        HStack(spacing: 10) {
            leading
                .padding(.vertical, scaled(8))
            Spacer(minLength: 12)
            trailing
                .padding(.trailing, -rowOutset(for: ControlSize.large.height))
        }
        .padding(.horizontal, settingsInset)
        .frame(minHeight: settingsRowHeight)
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
            .foregroundStyle(Color.themeMutedForeground)
            .fixedSize(horizontal: false, vertical: true)
            .padding(settingsInset)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card()
    }
}
#endif
