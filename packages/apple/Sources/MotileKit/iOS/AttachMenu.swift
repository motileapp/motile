#if os(iOS)
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// The button that attaches files to what is being written: from the photo library, from the
/// camera, or from the Files app. What is picked is copied into the client's own folder first, which
/// is where the core reads it from.
struct AttachMenu: View {
    @Environment(AppStore.self) private var store
    @State private var picksPhotos = false
    @State private var picksFiles = false
    @State private var takesPhoto = false
    @State private var photos: [PhotosPickerItem] = []

    var body: some View {
        Menu {
            Button {
                picksPhotos = true
            } label: {
                Label("Photo Library", systemImage: "photo.on.rectangle")
            }
            if UIImagePickerController.isSourceTypeAvailable(.camera) {
                Button {
                    takesPhoto = true
                } label: {
                    Label("Take Photo", systemImage: "camera")
                }
            }
            Button {
                picksFiles = true
            } label: {
                Label("Choose Files", systemImage: "folder")
            }
        } label: {
            Image(systemName: "plus")
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(Color.themeSecondary)
                .frame(width: 36, height: 36)
                .contentShape(Rectangle())
        }
        .accessibilityLabel("Attach files")
        .photosPicker(isPresented: $picksPhotos, selection: $photos, maxSelectionCount: 10, matching: .any(of: [.images, .videos]))
        .onChange(of: photos) { attachPhotos() }
        .fileImporter(isPresented: $picksFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            guard case .success(let urls) = result else { return }
            attach(copies: urls)
        }
        .fullScreenCover(isPresented: $takesPhoto) {
            Camera { image in
                takesPhoto = false
                guard let image else { return }
                DispatchQueue.global(qos: .userInitiated).async {
                    guard let jpeg = image.jpegData(compressionQuality: 0.9), let file = ImageFiles.saveForAttaching(jpeg, type: .jpeg) else { return }
                    DispatchQueue.main.async { store.attach([file]) }
                }
            }
            .ignoresSafeArea()
        }
    }

    private func attachPhotos() {
        let picked = photos
        guard !picked.isEmpty else { return }
        photos = []
        for item in picked {
            item.loadTransferable(type: PickedFile.self) { result in
                guard case .success(let file?) = result else { return }
                DispatchQueue.main.async { store.attach([file.url]) }
            }
        }
    }

    /// Files from other apps can only be read while the picker's permission lasts.
    private func attach(copies urls: [URL]) {
        DispatchQueue.global(qos: .userInitiated).async {
            let copies = urls.compactMap { url -> URL? in
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                return PickedFile.copy(url)
            }
            DispatchQueue.main.async { store.attach(copies) }
        }
    }
}

/// A file the user picked, copied to where the client can keep reading it.
struct PickedFile: Transferable {
    let url: URL

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .movie) { received in
            guard let copy = copy(received.file) else { throw CocoaError(.fileReadUnknown) }
            return PickedFile(url: copy)
        }
        FileRepresentation(importedContentType: .image) { received in
            guard let copy = copy(received.file) else { throw CocoaError(.fileReadUnknown) }
            return PickedFile(url: copy)
        }
    }

    /// Every file gets a folder of its own, so that two with one name are two files.
    static func copy(_ file: URL) -> URL? {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("motile-attachments", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let copy = folder.appendingPathComponent(file.lastPathComponent)
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try FileManager.default.copyItem(at: file, to: copy)
        } catch {
            return nil
        }
        return copy
    }
}

private struct Camera: UIViewControllerRepresentable {
    let done: (UIImage?) -> Void

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: UIImagePickerController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(done: done) }

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let done: (UIImage?) -> Void

        init(done: @escaping (UIImage?) -> Void) {
            self.done = done
        }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            done(info[.originalImage] as? UIImage)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            done(nil)
        }
    }
}
#endif
