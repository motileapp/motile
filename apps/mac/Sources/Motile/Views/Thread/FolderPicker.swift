import SwiftUI

/// Browses the folders of a host to pick the one a project lives in, or an image in a project's
/// folder to be its icon.
struct FolderPicker: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let host: Host
    var iconFor: Project?
    @State private var folder: RemoteFolder?
    @State private var path = ""
    @State private var selected: String?
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(iconFor.map { "Choose an icon for \($0.name)" } ?? "Add a project on \(host.name)")
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
                        store.addProject(hostID: host.id, path: target)
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
        .onAppear { load(iconFor?.path) }
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
        store.listFolder(hostID: host.id, path: path, icons: iconFor != nil) { result in
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
