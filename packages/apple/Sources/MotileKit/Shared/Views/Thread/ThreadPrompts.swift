import SwiftUI

extension View {
    /// Renaming a thread and asking before deleting one, which its menu starts.
    func threadPrompts(renaming: Binding<ThreadInfo?>, deleting: Binding<ThreadInfo?>) -> some View {
        modifier(ThreadPrompts(renaming: renaming, deleting: deleting))
    }
}

extension AppStore {
    /// What the open thread's title offers: finding in it, then what its row offers. Nothing while
    /// an agent is shown in its place.
    func titleActions(rename: @escaping (ThreadInfo) -> Void, delete: @escaping (ThreadInfo) -> Void) -> [[RowAction]] {
        guard let thread = selectedThread, shownAgent == nil else { return [] }
        let find = RowAction(title: "Find in thread") { self.openFind() }
        return [[find]] + rowActions(for: thread, rename: rename, delete: delete)
    }
}

private struct ThreadPrompts: ViewModifier {
    @Environment(AppStore.self) private var store
    @Binding var renaming: ThreadInfo?
    @Binding var deleting: ThreadInfo?
    #if os(iOS)
    @State private var newTitle = ""
    #endif

    #if os(macOS)
    func body(content: Content) -> some View {
        content
            .modal(item: $renaming) { thread in
                RenameThread(thread: thread)
            }
            .asking($deleting) { thread in
                Confirmation(
                    title: "Delete “\(thread.title)”?", message: store.deletionNote(thread),
                    answers: [Answer(title: "Delete", variant: .danger) { store.delete(thread) }]
                )
            }
    }
    #else
    func body(content: Content) -> some View {
        content
            .onChange(of: renaming?.id) {
                guard let thread = renaming else { return }
                newTitle = thread.title
            }
            .alert("Rename thread", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
                TextField("Title", text: $newTitle)
                Button("Rename") {
                    if let thread = renaming { store.rename(thread, to: newTitle) }
                    renaming = nil
                }
                Button("Cancel", role: .cancel) { renaming = nil }
            }
            .alert(
                "Delete “\(deleting?.title ?? "")”?",
                isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })
            ) {
                Button("Cancel", role: .cancel) { deleting = nil }
                Button("Delete", role: .destructive) {
                    if let thread = deleting { store.delete(thread) }
                    deleting = nil
                }
            } message: {
                Text(store.deletionNote(deleting))
            }
    }
    #endif
}

#if os(macOS)
private struct RenameThread: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let thread: ThreadInfo
    @State private var title: String

    init(thread: ThreadInfo) {
        self.thread = thread
        _title = State(initialValue: thread.title)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Rename Thread")
                .font(.ui(.lg, weight: .semibold))
            InputField("Title", text: $title)
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                ActionButton("Rename", variant: .primary, margin: .beside(leading: 4)) {
                    store.rename(thread, to: title)
                    close()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 400)
    }
}
#endif
