import SwiftUI

struct SettingsView: View {
    @Environment(AppStore.self) private var store
    @AppStorage("appearance") private var appearance = Appearance.system

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                SettingsSection("Account") {
                    SettingsRow {
                        Text("Signed in as")
                    } trailing: {
                        Text(store.account.signedIn ? store.account.email : "Not signed in")
                            .foregroundStyle(.secondary)
                        if store.account.signedIn {
                            Button("Sign Out") { store.signOut() }
                        }
                    }
                }

                SettingsSection("Updates") {
                    SettingsRow {
                        Text("Motile \(store.updater.current)")
                    } trailing: {
                        Button("Check for Updates") { store.updater.check(asked: true) }
                    }
                    if store.updater.state != .idle {
                        SettingsDivider()
                        AppUpdateRow(updater: store.updater)
                            .padding(.horizontal, settingsInset)
                            .padding(.vertical, 10)
                    }
                }

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

                if store.account.signedIn {
                    hosts
                    projects
                }
            }
            .padding(20)
        }
        .frame(width: 520, height: 560)
    }

    private var hosts: some View {
        SettingsSection("Hosts") {
            ForEach(store.hosts) { host in
                SettingsRow {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(host.name)
                        Text(description(of: host))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                } trailing: {
                    HostUpdateStatus(host: host) { EmptyView() }
                    Button("Remove") { store.removeHost(host) }
                }
                SettingsDivider()
            }
            SettingsRow {
                Button("Add a Host…") { store.showsAddHost = true }
            } trailing: {
                EmptyView()
            }
        }
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
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.head)
                    }
                } trailing: {
                    Menu("Icon") {
                        Button("Choose an Image…") { store.iconProject = project }
                        Button("Use the Icon in Its Folder") { store.setIcon(of: project, to: nil) }
                    }
                    .fixedSize()
                    Button("Remove") { store.removeProject(project) }
                }
                SettingsDivider()
            }
            SettingsRow {
                Button("Add a Project…") { store.showsFolderPicker = true }
                    .disabled(store.hosts.isEmpty)
            } trailing: {
                EmptyView()
            }
        }
    }

    private func description(of host: Host) -> String {
        let agents = host.agents.sorted { $0.key.rawValue < $1.key.rawValue }.map { "\($0.key.name) \($0.value)" }
        let installed = agents.isEmpty ? "no agent installed" : agents.joined(separator: ", ")
        switch host.state {
        case .connected: return "Connected · version \(host.version) · \(installed)"
        case .connecting: return "Connecting…"
        case .disconnected: return "Offline"
        case .refused: return "This host no longer accepts this Mac"
        }
    }
}

private let settingsInset: CGFloat = 12

/// A titled box of rows. The system's grouped form leaves more room under a row than over it,
/// so the rows are laid out here.
private struct SettingsSection<Content: View>: View {
    private let title: String
    private let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(size: 13, weight: .semibold))
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
        HStack(spacing: 10) {
            leading
            Spacer(minLength: 12)
            trailing
        }
        .padding(.horizontal, settingsInset)
        .padding(.vertical, 8)
        .frame(minHeight: 44)
    }
}

private struct SettingsDivider: View {
    var body: some View {
        Divider()
            .padding(.horizontal, settingsInset)
    }
}
