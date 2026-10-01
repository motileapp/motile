import SwiftUI

struct SettingsView: View {
    @Environment(AppStore.self) private var store
    @AppStorage("appearance") private var appearance = Appearance.system

    var body: some View {
        Form {
            Section("Account") {
                LabeledContent("Signed in as", value: store.account.signedIn ? store.account.email : "Not signed in")
                if store.account.signedIn {
                    Button("Sign Out") { store.signOut() }
                }
            }

            Section("Appearance") {
                Picker("Theme", selection: $appearance) {
                    ForEach(Appearance.allCases) { Text($0.label).tag($0) }
                }
                .pickerStyle(.segmented)
            }

            if store.account.signedIn {
                Section("Hosts") {
                    ForEach(store.hosts) { host in
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(host.name)
                                Text(description(of: host))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button("Remove") { store.removeHost(host) }
                        }
                    }
                    Button("Add a Host…") { store.showsAddHost = true }
                }

                Section("Projects") {
                    if store.projects.isEmpty {
                        Text("No projects yet").foregroundStyle(.secondary)
                    }
                    ForEach(store.projects) { project in
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(project.name)
                                Text(project.path)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button("Remove") { store.removeProject(project) }
                        }
                    }
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 520, height: 460)
    }

    private func description(of host: Host) -> String {
        let agents = host.agents.sorted { $0.key.rawValue < $1.key.rawValue }.map { "\($0.key.name) \($0.value)" }
        let installed = agents.isEmpty ? "no agent installed" : agents.joined(separator: ", ")
        switch host.state {
        case .connected: return "Connected · \(installed)"
        case .connecting: return "Connecting…"
        case .disconnected: return "Offline"
        case .refused: return "This host no longer accepts this Mac"
        }
    }
}
