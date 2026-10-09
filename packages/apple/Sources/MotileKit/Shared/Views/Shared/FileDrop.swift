import SwiftUI
import UniformTypeIdentifiers

/// Attaches what is dropped on the thread, and covers it while files are held over it.
private struct FileDrop: ViewModifier {
    @Environment(AppStore.self) private var store

    func body(content: Content) -> some View {
        @Bindable var store = store
        let shown = (store.dropTargeted || store.composerDropTargeted) && store.composerServer != nil
        content
            .overlay {
                if shown {
                    cover
                }
            }
            .animation(.easeOut(duration: 0.12), value: shown)
            .onDrop(of: [UTType.fileURL] + ImageFiles.attachable, isTargeted: $store.dropTargeted) { providers in
                store.attach(dropped: providers)
                return true
            }
    }

    private var cover: some View {
        VStack(spacing: 12 * Platform.scale) {
            Image(.paperclip, size: 28)
            Text("Drop files here")
                .font(.ui(size: 17, weight: .semibold))
        }
        .foregroundStyle(Color.themeText)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay {
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .strokeBorder(Color.themePrimary, style: StrokeStyle(lineWidth: 1.5, dash: [5, 4]))
                .padding(8)
        }
        .background(Color.themeBackground.opacity(0.9).ignoresSafeArea(edges: [.bottom, .horizontal]))
        .allowsHitTesting(false)
        .appearing()
    }
}

extension View {
    func takingDroppedFiles() -> some View {
        modifier(FileDrop())
    }
}
