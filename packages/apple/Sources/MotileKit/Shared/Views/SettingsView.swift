import SwiftUI

struct SettingsView: View {
    @Environment(AppStore.self) private var store
    #if os(macOS)
    @Environment(\.openWindow) private var openWindow
    #endif
    @AppStorage("appearance") private var appearance = Appearance.system
    @State private var setupProject: Project?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                SettingsSection("Account") {
                    SettingsRow {
                        Text("Signed in as")
                    } trailing: {
                        Text(store.account.signedIn ? store.account.email : "Not signed in")
                            .foregroundStyle(Color.themeSecondary)
                        if store.account.signedIn {
                            Button("Sign Out") { store.signOut() }
                        }
                    }
                }

                #if os(macOS)
                SettingsSection("Updates") {
                    SettingsRow {
                        Text("Motile \(store.updater.current)")
                    } trailing: {
                        Button("Check for Updates") { store.updater.check(asked: true) }
                            .disabled(store.updater.state == .checking)
                    }
                    if store.updater.state != .idle {
                        SettingsDivider()
                        AppUpdateRow(updater: store.updater)
                            .padding(.horizontal, settingsInset)
                            .padding(.vertical, 10)
                    }
                }
                #else
                SettingsSection("Version") {
                    SettingsRow {
                        Text("Motile \(store.updater.current)")
                    } trailing: {
                        EmptyView()
                    }
                }
                #endif

                SettingsSection("Appearance") {
                    SettingsRow {
                        Text("Theme")
                    } trailing: {
                        Picker("Theme", selection: $appearance) {
                            ForEach(Appearance.allCases) { Text($0.label).tag($0) }
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                        .fixedSize()
                    }
                }

                SettingsSection("Storage") {
                    SettingsRow {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Images and videos")
                            Text(storageDescription)
                                .font(.caption)
                                .foregroundStyle(Color.themeSecondary)
                        }
                    } trailing: {
                        Button("Clear") { store.clearMedia() }
                            .disabled((store.mediaStorage?.used ?? 0) == 0)
                    }
                }

                if store.account.signedIn {
                    servers
                    textGeneration
                    pullRequests
                    projects
                }
            }
            .padding(20)
        }
        #if os(macOS)
        .frame(width: 520, height: 560)
        #endif
        .onAppear { store.refreshMediaStorage() }
        .sheet(item: $setupProject) { project in
            SetupSheet(project: project)
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

    private var servers: some View {
        SettingsSection("Servers") {
            ForEach(store.servers) { server in
                SettingsRow {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(server.name)
                        Text(description(of: server))
                            .font(.caption)
                            .foregroundStyle(Color.themeSecondary)
                    }
                } trailing: {
                    ServerUpdateStatus(server: server) { EmptyView() }
                    Button("Remove") { store.removeServer(server) }
                }
                SettingsDivider()
            }
            SettingsRow {
                Button("Add a Server…") { store.showsAddServer = true }
            } trailing: {
                EmptyView()
            }
        }
    }

    /// The model that writes thread titles, branch names, commit messages and pull requests, and
    /// how it names branches, by server.
    @ViewBuilder private var textGeneration: some View {
        let servers = store.servers.filter { $0.state == .connected && $0.protocolVersion >= 4 }
        if !servers.isEmpty {
            SettingsSection("Text generation", caption: "The model that writes thread titles, branch names, commit messages and pull requests") {
                ForEach(servers) { server in
                    SettingsRow {
                        Image(.server, size: 13)
                            .foregroundStyle(Color.themeSecondary)
                        Text(server.name)
                    } trailing: {
                        Picker("Model", selection: textModel(of: server)) {
                            Text("Automatic").tag(String?.none)
                            ForEach(server.models) { model in
                                Text(model.name).tag(String?.some(model.id))
                            }
                        }
                        .labelsHidden()
                        .fixedSize()
                    }
                    if server.protocolVersion >= 6 {
                        BranchInstructionsEditor(server: server)
                    }
                    if server.id != servers.last?.id { SettingsDivider() }
                }
            }
        }
    }

    /// What each server does with pull requests by itself.
    @ViewBuilder private var pullRequests: some View {
        let servers = store.servers.filter { $0.state == .connected && $0.protocolVersion >= 9 }
        if !servers.isEmpty {
            SettingsSection("Pull requests", caption: "What your servers do once a thread's pull request merges") {
                ForEach(servers) { server in
                    if servers.count > 1 {
                        SettingsRow {
                            Image(.server, size: 13)
                                .foregroundStyle(Color.themeSecondary)
                            Text(server.name)
                                .fontWeight(.medium)
                        } trailing: {
                            EmptyView()
                        }
                    }
                    SettingsRow {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Mark the thread done")
                            Text("When its pull request merges or closes")
                                .font(.caption)
                                .foregroundStyle(Color.themeSecondary)
                        }
                    } trailing: {
                        Toggle("", isOn: Binding { server.doneOnMerge } set: {
                            store.setPullRequestSettings(doneOnMerge: $0, removeMergedWorktrees: server.removeMergedWorktrees, on: server)
                        })
                        .labelsHidden()
                        .toggleStyle(.switch)
                    }
                    SettingsDivider()
                    SettingsRow {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Remove the thread's worktree")
                            Text("After its pull request merges, if all of it is pushed")
                                .font(.caption)
                                .foregroundStyle(Color.themeSecondary)
                        }
                        .help("Its branch stays, and the worktree is made again if the thread goes on.")
                    } trailing: {
                        Toggle("", isOn: Binding { server.removeMergedWorktrees } set: {
                            store.setPullRequestSettings(doneOnMerge: server.doneOnMerge, removeMergedWorktrees: $0, on: server)
                        })
                        .labelsHidden()
                        .toggleStyle(.switch)
                    }
                    if server.id != servers.last?.id { SettingsDivider() }
                }
            }
        }
    }

    private func textModel(of server: Server) -> Binding<String?> {
        Binding { server.textModel } set: { store.setTextModel($0, on: server) }
    }

    private var projects: some View {
        SettingsSection("Projects") {
            ForEach(store.projects) { project in
                SettingsRow {
                    ProjectIcon(project: project, size: 26)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(project.name)
                        Text(project.path)
                            .font(.caption)
                            .foregroundStyle(Color.themeSecondary)
                            .lineLimit(1)
                            .truncationMode(.head)
                    }
                } trailing: {
                    Menu("Icon") {
                        Button("Choose an Image…") { store.iconProject = project }
                        Button("Use the Icon in Its Folder") { store.setIcon(of: project, to: nil) }
                    }
                    .fixedSize()
                    if (store.server(project.serverID)?.protocolVersion ?? 0) >= 6 {
                        Button("Setup…") { setupProject = project }
                            .help("The script that runs in each new worktree of \(project.name)")
                    }
                    Button("Remove") { store.removeProject(project) }
                }
                SettingsDivider()
            }
            SettingsRow {
                Button("Add a Project…") {
                    #if os(macOS)
                    openWindow(id: "main")
                    #else
                    store.showsSettings = false
                    #endif
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

private let settingsInset: CGFloat = 12

/// How a server's writer is told to name the branches it makes, to change and to put back.
private struct BranchInstructionsEditor: View {
    @Environment(AppStore.self) private var store
    let server: Server
    @State private var text = ""

    private var changed: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines) != server.branchInstructions }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text("Branch names")
                    .font(.caption)
                    .foregroundStyle(Color.themeSecondary)
                Spacer()
                Button("Reset") {
                    text = server.defaultBranchInstructions
                    store.setBranchInstructions(nil, on: server)
                }
                .disabled(server.branchInstructions == server.defaultBranchInstructions && !changed)
                Button("Save") { store.setBranchInstructions(text, on: server) }
                    .disabled(!changed)
            }
            .controlSize(.small)
            TextEditor(text: $text)
                .font(.ui(size: 12))
                .scrollContentBackground(.hidden)
                .padding(6)
                .frame(height: 64)
                .background(Color.themeField, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                .overlay { RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeBorder, lineWidth: 1) }
        }
        .padding(.horizontal, settingsInset)
        .padding(.bottom, 10)
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
            Text("Worktree setup for \(project.name)")
                .font(.ui(size: 13, weight: .semibold))
            Text("A shell script that runs in each new worktree before the agent starts there, to install what the work needs. $MOTILE_PROJECT is the project's folder, as in: cp \"$MOTILE_PROJECT/.env\" . && pnpm install")
                .font(.caption)
                .foregroundStyle(Color.themeSecondary)
                .fixedSize(horizontal: false, vertical: true)
            TextEditor(text: $script)
                .font(.ui(size: 12, design: .monospaced))
                .scrollContentBackground(.hidden)
                .padding(6)
                .frame(height: 140)
                .background(Color.themeField, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                .overlay { RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeBorder, lineWidth: 1) }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
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
        .presentationBackground(Color.themeSheet)
        #endif
        .onAppear { script = project.setup ?? "" }
    }
}

/// A titled box of rows. The system's grouped form leaves more room under a row than over it,
/// so the rows are laid out here.
private struct SettingsSection<Content: View>: View {
    private let title: String
    private let caption: String?
    private let content: Content

    init(_ title: String, caption: String? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.caption = caption
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.ui(size: 13, weight: .semibold))
                if let caption {
                    Text(caption)
                        .font(.caption)
                        .foregroundStyle(Color.themeSecondary)
                }
            }
            .padding(.horizontal, settingsInset)
            VStack(spacing: 0) {
                content
            }
            .background(Color.themeHover, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
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
                controls
            }
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 10) { leading }
                HStack(spacing: 10) { controls }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, settingsInset)
        .padding(.vertical, 8)
        .frame(minHeight: 44)
    }

    @ViewBuilder private var controls: some View {
        #if os(macOS)
        trailing
        #else
        Group { trailing }
            .buttonStyle(.bordered)
            .controlSize(.small)
        #endif
    }
}

private struct SettingsDivider: View {
    var body: some View {
        ThemeDivider()
            .padding(.horizontal, settingsInset)
    }
}
