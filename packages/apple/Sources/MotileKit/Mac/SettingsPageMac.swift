#if os(macOS)
import SwiftUI

/// One section of the settings: its groups of rows, and the way to the group a search picked.
struct SettingsPage: View {
    @Environment(AppStore.self) private var store
    let section: SettingsSection
    @AppStorage("appearance") private var appearance = Appearance.system
    @AppStorage(AppStore.steersKey) private var steers = AppStore.steersByDefault
    @AppStorage(AppStore.managesNewThreadsKey) private var managesNewThreads = false
    @State private var setupProject: Project?
    @State private var editedAccount: EditedAccount?
    @State private var removal: Removal?

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: SettingsGroupTitle.groupGap) {
                    switch section {
                    case .general: general
                    case .servers: servers
                    case .agentGateway: AgentGatewaySettings()
                    case .mcpGateway: McpGatewaySettings()
                    case .accounts: agentAccounts
                    case .projects: projects
                    case .textGeneration: textGeneration
                    case .pullRequests: pullRequests
                    case .keyboard: ShortcutSettings()
                    case .integrations: IntegrationSettings()
                    }
                }
                .frame(maxWidth: Theme.contentWidth)
                .padding(.horizontal, 20)
                .padding(.top, 20)
                .padding(.bottom, 80)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.immediately)
            .onChange(of: store.settingsTarget, initial: true) {
                guard let target = store.settingsTarget else { return }
                // A lazy list guesses the rows it hasn't built yet, so the scroll is set right once they are.
                withAnimation {
                    proxy.scrollTo(target, anchor: .center)
                } completion: {
                    withAnimation { proxy.scrollTo(target, anchor: .center) }
                }
                store.settingsTarget = nil
            }
        }
        .onAppear { store.refreshMediaStorage() }
        .confirmsRemoval($removal)
        .modal(item: $setupProject) { project in
            SetupSheet(project: project)
        }
        .modal(item: $editedAccount) { edited in
            AgentAccountSheet(server: edited.server, account: edited.account)
        }
        .modal(item: section == .servers ? Bindable(store).editedBackup : .constant(nil)) { edited in
            BackupSheet(bucket: edited.bucket)
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
                ActionMenu(appearance.label, variant: .outline, size: .large) {
                    ForEach(Appearance.allCases) { option in
                        Button(option.label) { appearance = option }
                    }
                }
            }
        }

        SettingsGroup("messages", "Messages") {
            SettingsRow {
                SettingsLabel(
                    "Sent while the agent works",
                    description: AppStore.steersDescription(steers))
            } trailing: {
                ActionMenu(steers ? "Steer" : "Queue", variant: .outline, size: .large) {
                    Button("Queue") { steers = false }
                    Button("Steer") { steers = true }
                }
            }
        }

        SettingsGroup("working-shelf", "Sidebar") {
            SettingsRow(control: Switch.height) {
                SettingsLabel("Working shelf", description: workingShelfDescription)
            } trailing: {
                Switch(isOn: Bindable(store).workingShelf)
            }
        }

        SettingsGroup("manage-new-threads", "Provider Limits") {
            SettingsRow(control: Switch.height) {
                SettingsLabel("Manage new threads automatically", description: managesNewThreadsDescription)
            } trailing: {
                Switch(isOn: $managesNewThreads)
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

        SettingsGroup("log", "Troubleshooting") {
            SettingsRow {
                SettingsLabel("Log", description: logDescription)
            } trailing: {
                ActionButton("Show in Finder", variant: .outline, size: .large, action: revealLog)
                    .takesFirstClick(revealLog)
            }
        }
    }

    private func revealLog() {
        guard let file = store.logFile else { return }
        MediaFiles.reveal(file)
    }

    @ViewBuilder private var servers: some View {
        SettingsGroup("servers", "Servers") {
            ForEach(store.servers) { server in
                SettingsRow {
                    SettingsLabel(server.name, description: server.settingsDescription, icon: .server) {
                        if let sync = server.sync { ServerSyncLine(sync: sync).padding(.top, 2) }
                    }
                } trailing: {
                    HStack(spacing: 0) {
                        ServerUpdateStatus(server: server, variant: .outline, size: .large, margin: .beside(trailing: settingsHalfGap)) { EmptyView() }
                        more("What to do with \(server.name)", margin: .beside(leading: settingsHalfGap)) {
                            MenuItem("Unlink Server", symbol: .unlink, role: .destructive) { removal = .server(server) }
                            MenuItem("Delete", symbol: .trash2, role: .destructive) { removal = .serverCompletely(server) }
                        }
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
        backup
        removedServers
        continuing
    }

    /// The account's S3 bucket, which every server backs up to and syncs through.
    @ViewBuilder private var backup: some View {
        let on = store.backup.bucket != nil
        SettingsGroup("backup", "Backup", caption: on ? "Your servers keep their threads, images and files in it, and sync through it." : nil) {
            SettingsRow {
                SettingsLabel("S3 Bucket", description: store.backup.description, icon: .cloudUpload, truncates: on ? .middle : nil)
            } trailing: {
                if on {
                    HStack(spacing: 0) {
                        ActionButton("Edit", variant: .outline, size: .large, margin: .beside(trailing: settingsHalfGap)) { store.editedBackup = EditedBackup(bucket: store.backup.bucket) }
                        more("What to do with the backup", margin: .beside(leading: settingsHalfGap)) {
                            MenuItem("Turn Off", symbol: .ban, role: .destructive) { removal = .backup }
                        }
                    }
                } else {
                    ActionButton("Set Up", variant: .outline, size: .large) { store.editedBackup = EditedBackup(bucket: nil) }
                }
            }
        }
    }

    /// The servers removed from the account, whose threads stay from the copies until they are
    /// deleted completely.
    @ViewBuilder private var removedServers: some View {
        let removed = store.removedServers
        if !removed.isEmpty {
            SettingsGroup("removed-servers", "Unlinked") {
                ForEach(removed) { server in
                    SettingsRow {
                        serverName(server)
                    } trailing: {
                        more("What to do with \(server.name)") {
                            MenuItem("Delete", symbol: .trash2, role: .destructive) { removal = .serverCompletely(server) }
                        }
                    }
                    if server.id != removed.last?.id { ThemeDivider() }
                }
            }
        }
    }

    /// What each server goes on with by itself: threads whose usage limit reset, and threads whose
    /// agents a restart cut off.
    @ViewBuilder private var continuing: some View {
        let servers = store.servers.filter { $0.state == .connected && store.canChooseRestart($0) }
        if !servers.isEmpty {
            SettingsGroup("continue-limits", "Continue after usage limits", caption: "A thread whose agent reached its usage limit goes on once the limit resets") {
                ForEach(servers) { server in
                    SettingsRow(control: Switch.height) {
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
                    SettingsRow(control: Switch.height) {
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
            SettingsGroup("merged", "Archive the thread after merge or close") {
                ForEach(servers) { server in
                    SettingsRow(control: Switch.height) {
                        serverName(server)
                    } trailing: {
                        Switch(isOn: Binding { server.doneOnMerge } set: {
                            store.setPullRequestSettings(doneOnMerge: $0, on: server)
                        })
                    }
                    if server.id != servers.last?.id { ThemeDivider() }
                }
            }
            SettingsGroup("worktrees", "Remove the thread's worktree after merge") {
                ForEach(servers) { server in
                    SettingsRow(control: Switch.height) {
                        serverName(server)
                    } trailing: {
                        Switch(isOn: Binding { server.removeMergedWorktrees } set: {
                            store.setPullRequestSettings(removeMergedWorktrees: $0, on: server)
                        })
                    }
                    if server.id != servers.last?.id { ThemeDivider() }
                }
            }
            let pulling = servers.filter(\.pullsDefaultBranch)
            if !pulling.isEmpty {
                SettingsGroup("pull", "Automatically pull", caption: pullDefaultBranchCaption) {
                    ForEach(pulling) { server in
                        SettingsRow(control: Switch.height) {
                            serverName(server)
                        } trailing: {
                            Switch(isOn: Binding { server.pullDefaultBranch } set: {
                                store.setPullRequestSettings(pullDefaultBranch: $0, on: server)
                            })
                        }
                        if server.id != pulling.last?.id { ThemeDivider() }
                    }
                }
            }
        }
    }

    /// The accounts each server's agents work with, a card for each server.
    @ViewBuilder private var agentAccounts: some View {
        if store.servers.isEmpty {
            SettingsNote("The accounts your agents work with are set on each of your servers, once you have one.")
        } else {
            SettingsGroup("agent-accounts", "Provider Accounts", caption: "Each account keeps its sign-in in a folder of its own. A thread works with one and can move to another.", carded: false) {
                VStack(spacing: 24) {
                    ForEach(store.servers) { server in
                        VStack(spacing: 0) {
                            agentAccounts(of: server)
                        }
                        .card()
                    }
                }
            }
        }
    }

    @ViewBuilder private func agentAccounts(of server: Server) -> some View {
        SettingsRow {
            HStack(spacing: 8) {
                Image(.server, size: .md)
                Text(server.name)
                    .font(.ui(.sm, weight: .medium))
            }
            .foregroundStyle(Color.themeForeground)
        } trailing: {
            if let reason = server.accountsUnavailable {
                Text(reason)
                    .font(.ui(.xs))
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        if server.accountsUnavailable == nil {
            let installed = Agent.allCases.filter { server.agents[$0] != nil }
            ThemeDivider()
            ForEach(server.agentAccounts.filter { installed.contains($0.agent) && $0.pin == nil }) { account in
                SettingsRow {
                    AgentIcon(agent: account.agent, size: .md)
                    SettingsLabel(
                        account.settingsTitle, description: account.settingsDescription(isDefault: server.isDefault(account)), proxied: account.gateway
                    )
                } trailing: {
                    if !account.gateway || !server.isDefault(account) {
                        more("What to do with the account") {
                            if !account.gateway {
                                MenuItem("Edit", symbol: .pencil) { editedAccount = EditedAccount(server: server, account: account) }
                            }
                            if !server.isDefault(account) {
                                MenuItem("Make Default", symbol: .circleCheck) { store.makeAgentAccountDefault(account, on: server) }
                            }
                            if !account.gateway {
                                Divider()
                                MenuItem("Remove Account", symbol: .trash2, role: .destructive) { removal = .account(account, on: server) }
                            }
                        }
                    }
                }
                ThemeDivider()
            }
            SettingsRow {
                ActionButton("Add Account", icon: .plus, variant: .outline, size: .large) {
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
                    ProjectIcon(project: project, size: .md)
                    SettingsLabel(project.name, description: project.path, truncates: .head)
                } trailing: {
                    more("What to do with \(project.name)") {
                        MenuItem("Choose an Icon", symbol: .image) { store.openPanel(.icon(project.id)) }
                        if (store.server(project.serverID)?.protocolVersion ?? 0) >= 6 {
                            MenuItem("Worktree Setup", symbol: .terminal) { setupProject = project }
                        }
                        Divider()
                        MenuItem("Remove Project", symbol: .trash2, role: .destructive) { removal = .project(project) }
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
    private func more<Items: View>(_ help: String, margin: EdgeInsets = EdgeInsets(), @ViewBuilder items: () -> Items) -> some View {
        ActionMenu(icon: .ellipsis, help: help, size: .large, margin: margin, content: items)
    }
}

let settingsInset: CGFloat = 14
/// How tall a list that keeps growing, as a log, is before it scrolls.
let settingsListHeight: CGFloat = 420
let settingsRowHeight: CGFloat = scaled(48)
private let settingsRowSpacing: CGFloat = 10

/// What stands a control this tall as far from its row's right as from its top and bottom.
func rowOutset(for height: CGFloat, in row: CGFloat = settingsRowHeight) -> CGFloat {
    settingsInset - (row - height) / 2
}

/// Half the room between the lights of two controls side by side at a row's end, which each
/// takes as its own.
let settingsHalfGap = settingsRowSpacing / 2

/// How a server's writer is told to name the branches it makes, to change and to put back.
private struct BranchInstructionsEditor: View {
    @Environment(AppStore.self) private var store
    let server: Server
    @State private var text = ""

    private var changed: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines) != server.branchInstructions }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            TextArea("How to name a branch", text: $text, maxLines: 16)
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Reset", variant: .outline, size: .large, margin: .beside(trailing: 4)) {
                    text = server.defaultBranchInstructions
                    store.setBranchInstructions(nil, on: server)
                }
                .disabled(server.branchInstructions == server.defaultBranchInstructions && !changed)
                ActionButton("Save", variant: .primary, size: .large, margin: .beside(leading: 4)) { store.setBranchInstructions(text, on: server) }
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
    @Environment(\.closeModal) private var close
    let project: Project
    @State private var script = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Worktree Setup for \(project.name)")
                .font(.ui(.lg, weight: .semibold))
            Text("A shell script that runs in each new worktree before the agent starts there, to install what the work needs. $MOTILE_PROJECT is the project's folder, as in: cp \"$MOTILE_PROJECT/.env\" . && pnpm install")
                .font(.ui(.xxs))
                .foregroundStyle(Color.themeMutedForeground)
                .fixedSize(horizontal: false, vertical: true)
            TextArea("cp \"$MOTILE_PROJECT/.env\" . && pnpm install", text: $script, monospaced: true, lines: 5, maxLines: 14)
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, margin: .beside(leading: 4)) {
                    store.setSetup(of: project, to: script)
                    close()
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 440)
        .onAppear { script = project.setup ?? "" }
    }
}

struct SettingsGroup<Content: View>: View {
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
        VStack(alignment: .leading, spacing: SettingsGroupTitle.gap) {
            SettingsGroupTitle(title, caption: caption)
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

/// The title of a group of settings, with a caption under it when it has one.
struct SettingsGroupTitle: View {
    /// The room between the title and the group's rows.
    static let gap: CGFloat = 12
    /// The room between one group and the next.
    static let groupGap: CGFloat = 22

    let title: String
    var caption: String?

    init(_ title: String, caption: String? = nil) {
        self.title = title
        self.caption = caption
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.ui(.sm))
                .foregroundStyle(Color.themeForeground)
            if let caption {
                Text(caption)
                    .font(.ui(.xs))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal, 4)
    }
}

/// What a row is about: its name, and under it what it does.
struct SettingsLabel<Footer: View>: View {
    let title: String
    var description: String?
    var icon: Symbol?
    /// A description that is cut on one line instead of wrapped: a path at its start, a sentence at its end.
    var truncates: Text.TruncationMode?
    /// What the title names runs through the gateway, which its symbol after it says.
    var proxied = false
    /// What stands under the description.
    @ViewBuilder var footer: Footer

    init(
        _ title: String, description: String? = nil, icon: Symbol? = nil, truncates: Text.TruncationMode? = nil, proxied: Bool = false,
        @ViewBuilder footer: () -> Footer
    ) {
        self.title = title
        self.description = description
        self.icon = icon
        self.truncates = truncates
        self.proxied = proxied
        self.footer = footer()
    }

    var body: some View {
        HStack(spacing: 8) {
            if let icon {
                Image(icon, size: .md)
                    .foregroundStyle(Color.themeMutedForeground)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text(verbatim: title)
                    .gatewayMarked(proxied, fontSize: .sm)
                    .font(.ui(.sm, weight: .medium))
                if let description {
                    Text(description)
                        .font(.ui(.xs))
                        .foregroundStyle(Color.themeMutedForeground)
                        .lineLimit(truncates == nil ? nil : 1)
                        .truncationMode(truncates ?? .tail)
                        .fixedSize(horizontal: false, vertical: truncates == nil)
                }
                footer
            }
        }
    }
}

extension SettingsLabel where Footer == EmptyView {
    init(_ title: String, description: String? = nil, icon: Symbol? = nil, truncates: Text.TruncationMode? = nil, proxied: Bool = false) {
        self.init(title, description: description, icon: icon, truncates: truncates, proxied: proxied) { EmptyView() }
    }
}

/// What the row is about on the left, its controls on the right. Every row is as tall as the
/// tallest, with the same room above and below.
struct SettingsRow<Leading: View, Trailing: View>: View {
    /// How tall the trailing control is, which stands as far from the row's right as from its top and bottom.
    let control: CGFloat
    let leading: Leading
    let trailing: Trailing
    @State private var height = settingsRowHeight

    init(control: CGFloat = ControlSize.large.height, @ViewBuilder leading: () -> Leading, @ViewBuilder trailing: () -> Trailing) {
        self.control = control
        self.leading = leading()
        self.trailing = trailing()
    }

    var body: some View {
        HStack(spacing: settingsRowSpacing) {
            leading
                .padding(.vertical, scaled(8))
            Spacer(minLength: 12)
            HStack(spacing: settingsRowSpacing) { trailing }
                .padding(.trailing, -rowOutset(for: control, in: height))
        }
        .padding(.horizontal, settingsInset)
        .frame(minHeight: settingsRowHeight)
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height = $0 }
    }
}

/// Said on a page with nothing to set yet.
struct SettingsNote: View {
    let text: String

    init(_ text: String) {
        self.text = text
    }

    var body: some View {
        Text(text)
            .font(.ui(.sm))
            .foregroundStyle(Color.themeMutedForeground)
            .fixedSize(horizontal: false, vertical: true)
            .padding(settingsInset)
            .frame(maxWidth: .infinity, alignment: .leading)
            .card()
    }
}
#endif
