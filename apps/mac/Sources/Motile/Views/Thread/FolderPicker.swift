import SwiftUI

/// Browses the folders of a host to pick the one a project lives in.
struct FolderPicker: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let host: Host
    @State private var folder: RemoteFolder?
    @State private var path = ""
    @State private var selected: String?
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Add a project on \(host.name)")
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
                TextField("Path on the host", text: $path)
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
            }
            .listStyle(.inset(alternatesRowBackgrounds: true))
            .frame(height: 300)
            .overlay {
                if let error {
                    Text(error).foregroundStyle(Color.themeDanger).font(.system(size: 12)).padding()
                } else if folder?.folders.isEmpty == true {
                    Text("No folders in here").foregroundStyle(Color.themeTertiary).font(.system(size: 12))
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
                Button("Add Project") {
                    store.addProject(hostID: host.id, path: target)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(target.isEmpty)
            }
            .padding(14)
        }
        .frame(width: 520)
        .onAppear { load(nil) }
    }

    /// The folder that would be added: the selected one, or the one being browsed.
    private var target: String {
        guard let selected else { return folder?.path ?? path }
        return child(selected)
    }

    private func child(_ name: String) -> String {
        let base = folder?.path ?? ""
        return base.hasSuffix("/") ? base + name : base + "/" + name
    }

    private func load(_ path: String?) {
        store.listFolder(hostID: host.id, path: path) { result in
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
