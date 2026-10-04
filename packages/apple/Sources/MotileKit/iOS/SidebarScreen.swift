#if os(iOS)
import SwiftUI

/// The sidebar: the drafts, every active thread on every server, the ones marked done, the
/// servers and the account. On a phone it lies under the thread; in a wide window beside it.
struct SidebarScreen: View {
    @Environment(AppStore.self) private var store
    @Environment(Drawer.self) private var drawer
    /// The sidebar lies under the thread, which comes back over it when something is opened.
    var underThread = true
    @AppStorage("sidebar.doneExpanded") private var doneExpanded = false
    @State private var renaming: ThreadInfo?
    @State private var newTitle = ""
    @State private var deleting: ThreadInfo?
    @State private var search = ""
    /// The row a swipe has slid aside, by its id in the list.
    @State private var swiped: String?
    /// Why a swipe couldn't mark a thread done: its agent is still at it.
    @State private var busy = ""
    @State private var showsBusy = false

    var body: some View {
        let active = store.activeThreads.filter { store.matches($0, search: search) }
        let done = store.doneThreads.filter { store.matches($0, search: search) }
        let projects = store.projectsByID
        let selection = store.selection
        VStack(spacing: 0) {
            header
            ScrollView {
                LazyVStack(spacing: 0) {
                    DraftRows(search: search, open: open)
                    ForEach(active) { thread in
                        ThreadRow(
                            thread: thread, project: projects[thread.projectID]?.seen(from: thread),
                            selected: selection == .thread(thread.id), rename: beginRename, delete: { deleting = $0 }, open: open
                        )
                        .equatable()
                        .rowSwipe(
                            "checkmark", "Mark Done", tint: thread.busy ? .themeSecondary : .themeSuccess, size: 36,
                            leaves: !thread.busy, isOpen: swipe(thread.id)
                        ) {
                            markDone(thread)
                        }
                    }
                    if active.isEmpty {
                        Text(!search.isEmpty ? "No threads found" : done.isEmpty ? "No threads yet" : "No active threads")
                            .font(.ui(size: 13))
                            .foregroundStyle(Color.themeTertiary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, sidebarRowInset + 8)
                            .padding(.vertical, 10)
                    }
                    if !done.isEmpty {
                        doneShelf(done, projects: projects, selection: selection)
                    }
                }
                .padding(.bottom, 12)
            }
            .scrollDismissesKeyboard(.immediately)
            .onScrollPhaseChange { _, phase in
                if phase.isScrolling { swiped = nil }
            }
            if let undo = store.undo {
                UndoRow(notice: undo)
                    .transition(.opacity)
            }
            footer
        }
        .contentShape(Rectangle())
        .onTapGesture { Platform.endEditing() }
        .animation(.easeOut(duration: 0.15), value: store.undo)
        .alert(busy, isPresented: $showsBusy) {
            Button("OK", role: .cancel) {}
        } message: {
            Text("Stop it first, then mark it done.")
        }
        .alert("Rename thread", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("Title", text: $newTitle)
            Button("Rename") {
                if let thread = renaming { store.rename(thread, to: newTitle) }
                renaming = nil
            }
            Button("Cancel", role: .cancel) { renaming = nil }
        }
        .confirmationDialog(
            "Delete “\(deleting?.title ?? "")”?",
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                if let thread = deleting { store.delete(thread) }
                deleting = nil
            }
        } message: {
            Text(store.deletionNote(deleting))
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("Motile")
                .font(.system(size: 28, weight: .semibold))
                .foregroundStyle(Color.themeText)
            Spacer()
            Button {
                store.openPanel(.commands)
            } label: {
                Image(systemName: "command")
                    .font(.system(size: 17, weight: .medium))
                    .foregroundStyle(Color.themeText)
                    .frame(width: 42, height: 42)
                    .contentShape(Circle())
            }
            .buttonStyle(.plain)
            .glassButton(in: Circle())
            .accessibilityLabel("Commands")
        }
        .padding(.leading, sidebarRowInset + 8)
        .padding(.trailing, sidebarRowInset + 2)
        .padding(.top, 6)
        .padding(.bottom, 12)
    }

    /// The threads marked done: a line that opens into their list, under the active ones.
    @ViewBuilder
    private func doneShelf(_ done: [ThreadInfo], projects: [String: Project], selection: Selection) -> some View {
        let expanded = !search.isEmpty || doneExpanded
        Button {
            doneExpanded.toggle()
        } label: {
            HStack(spacing: 7) {
                Image(systemName: "chevron.right")
                    .font(.ui(size: 10, weight: .semibold))
                    .rotationEffect(.degrees(expanded ? 90 : 0))
                    .frame(width: 14)
                Text("Done")
                    .font(.ui(size: 12, weight: .medium))
                Spacer()
                Text("\(done.count)")
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeTertiary)
                    .monospacedDigit()
            }
            .foregroundStyle(Color.themeSecondary)
            .padding(.horizontal, sidebarRowInset + 8)
            .frame(height: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.top, 8)
        if expanded {
            ForEach(done, id: \.doneRowID) { thread in
                DoneRow(
                    thread: thread, project: projects[thread.projectID], selected: selection == .thread(thread.id),
                    rename: beginRename, delete: { deleting = $0 }, open: open
                )
                .equatable()
                .frame(height: 44)
                .rowSwipe("arrow.uturn.backward", "Mark Undone", tint: .themeSecondary, size: 28, isOpen: swipe(thread.doneRowID)) {
                    store.setDone([thread.id], done: false)
                }
            }
        }
    }

    /// The servers and how the client reaches them, and under them the account, the search and
    /// the way to a new thread.
    private var footer: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(store.servers) { ServerLine(server: $0) }
            HStack(spacing: 8) {
                Menu {
                    Section(store.account.email) {
                        Button {
                            store.showsSettings = true
                        } label: {
                            Label("Settings", systemImage: "gearshape")
                        }
                        Button {
                            store.addProject()
                        } label: {
                            Label("Add a Project", systemImage: "folder.badge.plus")
                        }
                        Button {
                            store.showsAddServer = true
                        } label: {
                            Label("Add a Server", systemImage: "server.rack")
                        }
                    }
                    Button(role: .destructive) {
                        store.signOut()
                    } label: {
                        Label("Sign Out", systemImage: "rectangle.portrait.and.arrow.right")
                    }
                } label: {
                    Text(initial)
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(Color.themeText)
                        .frame(width: 46, height: 46)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .glassButton(in: Circle())
                .accessibilityLabel("Account")
                SearchField(text: $search, bare: true)
                    .padding(.horizontal, 6)
                    .frame(height: 46)
                    .glassButton(in: Capsule())
                Button {
                    showThread()
                    store.newThread()
                } label: {
                    Image(systemName: "square.and.pencil")
                        .font(.system(size: 17, weight: .medium))
                        .foregroundStyle(Color.themeText)
                        .frame(width: 46, height: 46)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
                .glassButton(in: Circle())
                .disabled(store.projects.isEmpty)
                .accessibilityLabel("New thread")
            }
            .padding(.top, 6)
        }
        .padding(.horizontal, sidebarRowInset + 8)
        .padding(.top, 10)
        .padding(.bottom, 8)
        .background(alignment: .top) {
            Rectangle()
                .fill(Color.themeBorder)
                .frame(height: 1)
        }
    }

    private var initial: String {
        String(store.account.email.prefix(1)).uppercased()
    }

    /// Opens a thread or a draft, and puts the sidebar away where it lies under the thread.
    private func open(_ selection: Selection) {
        store.select(selection)
        showThread()
    }

    private func showThread() {
        guard underThread else { return }
        store.sidePanel.isOpen = false
        drawer.isOpen = false
    }

    private func markDone(_ thread: ThreadInfo) {
        guard !thread.busy else {
            busy = thread.running ? "Still working" : "Still monitoring"
            showsBusy = true
            return
        }
        store.setDone([thread.id], done: true, fromSidebar: true)
    }

    private func swipe(_ row: String) -> Binding<Bool> {
        Binding(get: { swiped == row }, set: { open in
            if open { swiped = row } else if swiped == row { swiped = nil }
        })
    }

    private func beginRename(_ thread: ThreadInfo) {
        newTitle = thread.title
        renaming = thread
    }
}

private extension ThreadInfo {
    /// Not the id its active row has in the same list, or the list keeps that row for it.
    var doneRowID: String { "done:\(id)" }
}
#endif
