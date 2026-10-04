import SwiftUI

/// Browses the folders of a server to pick an image as a project's icon.
struct FolderPicker: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let iconFor: Project
    private let serverID: String
    @State private var folder: RemoteFolder?
    @State private var path = ""
    @State private var selected: String?
    @State private var error: String?

    init(server: Server, iconFor: Project) {
        serverID = server.id
        self.iconFor = iconFor
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Choose an icon for \(iconFor.name)")
                .font(.ui(size: 15, weight: .semibold))
                .padding([.horizontal, .top], 18)
            HStack(spacing: 8) {
                Button {
                    if let parent = folder?.parent { load(parent) }
                } label: {
                    Image(.chevronUp, size: 13)
                }
                .disabled(folder?.parent == nil)
                .help("Enclosing folder")
                TextField("Path on the server", text: $path)
                    .textFieldStyle(.roundedBorder)
                    .font(.ui(size: 12.5, design: .monospaced))
                    .onSubmit { load(path) }
            }
            .padding(.horizontal, 18)
            .padding(.top, 12)

            List(selection: $selected) {
                ForEach(folder?.folders ?? [], id: \.self) { name in
                    Label(name, symbol: .folder)
                        .tag(name)
                        .contentShape(Rectangle())
                        .onTapGesture(count: 2) { load(child(name)) }
                        .onTapGesture { selected = name }
                }
                ForEach(folder?.files ?? [], id: \.self) { name in
                    Label(name, symbol: .image)
                        .tag(name)
                        .contentShape(Rectangle())
                        .onTapGesture(count: 2) { useAsIcon(name) }
                        .onTapGesture { selected = name }
                }
            }
            .modifier(FolderListStyle())
            .frame(height: 300)
            .overlay {
                if let error {
                    Text(error).foregroundStyle(Color.themeDanger).font(.ui(size: 12)).padding()
                } else if let folder, folder.folders.isEmpty, folder.files.isEmpty {
                    Text("No folders or images in here")
                        .foregroundStyle(Color.themeTertiary)
                        .font(.ui(size: 12))
                }
            }
            .padding(.top, 12)

            ThemeDivider()
            HStack {
                Text(target)
                    .font(.ui(size: 12, design: .monospaced))
                    .foregroundStyle(Color.themeSecondary)
                    .lineLimit(1)
                    .truncationMode(.head)
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Use as Icon") {
                    if let selectedImage { useAsIcon(selectedImage) }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(selectedImage == nil)
            }
            .padding(14)
        }
        #if os(macOS)
        .frame(width: 520)
        #endif
        .onAppear { load(iconFor.path) }
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
        store.setIcon(of: iconFor, to: child(name))
        dismiss()
    }

    private func load(_ path: String?) {
        store.listFolder(serverID: serverID, path: path, icons: true) { result in
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

private struct FolderListStyle: ViewModifier {
    func body(content: Content) -> some View {
        #if os(macOS)
        content.listStyle(.inset(alternatesRowBackgrounds: true))
        #else
        content.listStyle(.plain).scrollContentBackground(.hidden)
        #endif
    }
}
