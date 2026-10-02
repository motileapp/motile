import SwiftUI

/// Browses the folders of a server to pick the one a project lives in, or an image in a project's
/// folder to be its icon.
struct FolderPicker: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let iconFor: Project?
    @State private var serverID: String
    @State private var folder: RemoteFolder?
    @State private var path = ""
    @State private var selected: String?
    @State private var error: String?

    init(server: Server, iconFor: Project? = nil) {
        _serverID = State(initialValue: server.id)
        self.iconFor = iconFor
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            title
                .font(.system(size: 15, weight: .semibold))
                .padding([.horizontal, .top], 18)
            HStack(spacing: 8) {
                Button {
                    if let parent = folder?.parent { load(parent) }
                } label: {
                    Image(systemName: "chevron.up")
                }
                .disabled(folder?.parent == nil)
                .help("Enclosing folder")
                TextField("Path on the server", text: $path)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 12.5, design: .monospaced))
                    .onSubmit { load(path) }
            }
            .padding(.horizontal, 18)
            .padding(.top, 12)

            List(selection: $selected) {
                ForEach(folder?.folders ?? [], id: \.self) { name in
                    Label(name, systemImage: "folder")
                        .tag(name)
                        .contentShape(Rectangle())
                        .onTapGesture(count: 2) { load(child(name)) }
                        .onTapGesture { selected = name }
                }
                ForEach(folder?.files ?? [], id: \.self) { name in
                    Label(name, systemImage: "photo")
                        .tag(name)
                        .contentShape(Rectangle())
                        .onTapGesture(count: 2) { useAsIcon(name) }
                        .onTapGesture { selected = name }
                }
            }
            .listStyle(.inset(alternatesRowBackgrounds: true))
            .frame(height: 300)
            .overlay {
                if let error {
                    Text(error).foregroundStyle(Color.themeDanger).font(.system(size: 12)).padding()
                } else if let folder, folder.folders.isEmpty, folder.files.isEmpty {
                    Text(iconFor == nil ? "No folders in here" : "No folders or images in here")
                        .foregroundStyle(Color.themeTertiary)
                        .font(.system(size: 12))
                }
            }
            .padding(.top, 12)

            Divider()
            HStack {
                Text(target)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(Color.themeSecondary)
                    .lineLimit(1)
                    .truncationMode(.head)
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                if iconFor == nil {
                    Button("Add Project") {
                        store.addProject(serverID: serverID, path: target)
                        dismiss()
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(target.isEmpty)
                } else {
                    Button("Use as Icon") {
                        if let selectedImage { useAsIcon(selectedImage) }
                    }
                    .keyboardShortcut(.defaultAction)
                    .disabled(selectedImage == nil)
                }
            }
            .padding(14)
        }
        .frame(width: 520)
        .onAppear(perform: start)
    }

    @ViewBuilder private var title: some View {
        let name = store.server(serverID)?.name ?? ""
        if let iconFor {
            Text("Choose an icon for \(iconFor.name)")
        } else if store.servers.count > 1 {
            HStack(spacing: 6) {
                Text("Add a project on")
                Menu {
                    ForEach(store.servers) { server in
                        Button {
                            show(server)
                        } label: {
                            Label(server.state == .connected ? server.name : "\(server.name) (offline)", systemImage: "server.rack")
                        }
                        .disabled(server.state != .connected)
                    }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "server.rack")
                            .font(.system(size: 12, weight: .medium))
                        Text(name)
                        Image(systemName: "chevron.down")
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundStyle(Color.themeTertiary)
                    }
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .contentShape(Rectangle())
                }
                .menuStyle(.button)
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .fixedSize()
                .hoverHighlight(radius: 6)
                .padding(.horizontal, -6)
            }
        } else {
            Text("Add a project on \(name)")
        }
    }

    /// Starts on the given server, or on one that is connected when it isn't.
    private func start() {
        if iconFor == nil, store.server(serverID)?.state != .connected,
           let connected = store.servers.first(where: { $0.state == .connected }) {
            serverID = connected.id
        }
        load(iconFor?.path)
    }

    private func show(_ server: Server) {
        guard server.id != serverID else { return }
        serverID = server.id
        folder = nil
        path = ""
        selected = nil
        error = nil
        load(nil)
    }

    /// What would be chosen: the selected folder or image, or the folder being browsed.
    private var target: String {
        guard let selected else { return folder?.path ?? path }
        return child(selected)
    }

    private var selectedImage: String? {
        guard let selected, folder?.files.contains(selected) == true else { return nil }
        return selected
    }

    private func child(_ name: String) -> String {
        let base = folder?.path ?? ""
        return base.hasSuffix("/") ? base + name : base + "/" + name
    }

    private func useAsIcon(_ name: String) {
        guard let iconFor else { return }
        store.setIcon(of: iconFor, to: child(name))
        dismiss()
    }

    private func load(_ path: String?) {
        let browsed = serverID
        store.listFolder(serverID: browsed, path: path, icons: iconFor != nil) { result in
            guard browsed == serverID else { return }
            switch result {
            case .success(let folder):
                self.folder = folder
                self.path = folder.path
                selected = nil
                error = nil
            case .failure(let failure):
                error = failure.message
            }
        }
    }
}
