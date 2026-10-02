import SwiftUI

/// Every thread on every host in one list: the active ones, then the ones marked done.
struct SidebarView: View {
    @Environment(AppStore.self) private var store
    @AppStorage("sidebar.doneExpanded") private var doneExpanded = false
    @State private var doneLimit = 10
    @State private var renaming: ThreadInfo?
    @State private var newTitle = ""
    @State private var deleting: ThreadInfo?
    @State private var search = ""

    var body: some View {
        let active = store.activeThreads.filter(matches)
        let done = store.doneThreads.filter(matches)
        List(selection: selection) {
            Section {
                ForEach(active) { thread in
                    ThreadRow(thread: thread, rename: beginRename, delete: { deleting = $0 })
                        .tag(Selection.thread(thread.id))
                }
                if active.isEmpty {
                    Text(!search.isEmpty ? "No threads found" : done.isEmpty ? "No threads yet" : "No active threads")
                        .font(.system(size: 12))
                        .foregroundStyle(Color.themeTertiary)
                        .padding(.vertical, 6)
                        .selectionDisabled()
                }
            }

            if !done.isEmpty {
                Section(isExpanded: search.isEmpty ? $doneExpanded : .constant(true)) {
                    ForEach(done.prefix(doneLimit)) { thread in
                        DoneRow(thread: thread, rename: beginRename, delete: { deleting = $0 })
                            .tag(Selection.thread(thread.id))
                    }
                    if done.count > doneLimit {
                        Button("Show \(min(done.count - doneLimit, 25)) more") { doneLimit += 25 }
                            .buttonStyle(.plain)
                            .font(.system(size: 12))
                            .foregroundStyle(Color.themeSecondary)
                            .selectionDisabled()
                    }
                } header: {
                    Text(doneExpanded || !search.isEmpty ? "Done" : "Done (\(done.count))")
                }
            }
        }
        .listStyle(.sidebar)
        .searchable(text: $search, placement: .sidebar, prompt: "Search")
        .safeAreaInset(edge: .bottom, spacing: 0) {
            SidebarFooter()
        }
        .toolbar {
            ToolbarItem {
                Button {
                    store.startNewThread()
                } label: {
                    Label("New Thread", systemImage: "square.and.pencil")
                }
                .help("New thread (⌘N)")
            }
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
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } })
        ) {
            Button("Delete", role: .destructive) {
                if let thread = deleting { store.delete(thread) }
                deleting = nil
            }
        } message: {
            Text("The thread and its transcript are removed from the host. Files the agent changed stay as they are.")
        }
    }

    private var selection: Binding<Selection?> {
        Binding(
            get: { store.selection == .newThread ? nil : store.selection },
            set: { if let new = $0 { store.select(new) } }
        )
    }

    /// Whether the thread's title or project matches what is being searched for.
    private func matches(_ thread: ThreadInfo) -> Bool {
        guard !search.isEmpty else { return true }
        let project = store.project(thread.projectID)?.name ?? ""
        return thread.title.localizedCaseInsensitiveContains(search) || project.localizedCaseInsensitiveContains(search)
    }

    private func beginRename(_ thread: ThreadInfo) {
        newTitle = thread.title
        renaming = thread
    }
}

private struct ThreadMenu: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void

    var body: some View {
        if thread.isDone {
            Button("Mark Undone") { store.setDone([thread.id], done: false) }
        } else {
            Button("Mark Done") { store.setDone([thread.id], done: true, fromSidebar: true) }
                .disabled(thread.running)
        }
        Button("Rename…") { rename(thread) }
        Divider()
        Button("Delete…", role: .destructive) { delete(thread) }
    }
}

/// An active thread: its project and what it is doing on the first line, its title on the second.
private struct ThreadRow: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    @State private var hovering = false

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            HStack(spacing: 5) {
                Image(systemName: "folder")
                    .font(.system(size: 10, weight: .medium))
                Text(projectName)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                Spacer(minLength: 6)
                if hovering && !thread.running {
                    Button {
                        store.setDone([thread.id], done: true, fromSidebar: true)
                    } label: {
                        Label("Done", systemImage: "checkmark")
                            .font(.system(size: 11, weight: .medium))
                    }
                    .buttonStyle(.plain)
                    .help("Mark done")
                } else {
                    ThreadStatus(thread: thread)
                }
            }
            .foregroundStyle(.secondary)
            .frame(height: 15)

            Text(thread.title)
                .font(.system(size: 13, weight: .medium))
                .lineLimit(1)
        }
        .padding(.vertical, 5)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
    }

    private var projectName: String {
        let project = store.project(thread.projectID)?.name ?? URL(fileURLWithPath: thread.cwd).lastPathComponent
        guard store.hosts.count > 1, let host = store.host(thread.hostID) else { return project }
        return "\(project) · \(host.name)"
    }
}

/// A thread that is done: one quiet line.
private struct DoneRow: View {
    @Environment(AppStore.self) private var store
    let thread: ThreadInfo
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 6) {
            Text(thread.title)
                .font(.system(size: 13))
                .lineLimit(1)
                .foregroundStyle(.secondary)
            Spacer(minLength: 6)
            if hovering {
                Button {
                    store.setDone([thread.id], done: false)
                } label: {
                    Image(systemName: "arrow.uturn.backward")
                        .font(.system(size: 11, weight: .medium))
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help("Mark undone")
            } else {
                TimelineView(.periodic(from: .now, by: 30)) { context in
                    Text(Time.ago(thread.doneAt ?? thread.updatedAt, now: context.date.timeIntervalSince1970))
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
    }
}

/// What a thread is up to, or how long ago it last was.
private struct ThreadStatus: View {
    let thread: ThreadInfo

    var body: some View {
        if thread.running {
            TimelineView(.periodic(from: .now, by: 1)) { context in
                label("Working \(Time.elapsed(since: thread.updatedAt, now: context.date.timeIntervalSince1970))", "circle.dashed", Color.themeWorking)
            }
        } else if thread.needsApproval {
            label("Approval", "questionmark.circle", Color.themeWarning)
        } else if thread.unread {
            label("Unread", "circle.fill", Color.themePrimary)
        } else {
            TimelineView(.periodic(from: .now, by: 30)) { context in
                Text(Time.ago(thread.updatedAt, now: context.date.timeIntervalSince1970))
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func label(_ text: String, _ symbol: String, _ color: Color) -> some View {
        HStack(spacing: 3) {
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
            Text(text)
                .font(.system(size: 11, weight: .medium))
                .monospacedDigit()
        }
        .foregroundStyle(color)
    }
}

/// The hosts and how the app reaches them, the account, and the offer to undo.
private struct SidebarFooter: View {
    @Environment(AppStore.self) private var store
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let undo = store.undo {
                HStack(spacing: 6) {
                    Text(undo.text)
                        .foregroundStyle(.secondary)
                    Button("Undo") { store.performUndo() }
                        .buttonStyle(.link)
                    Spacer()
                }
                .font(.system(size: 12))
                .transition(.opacity)
            }
            ForEach(store.hosts) { host in
                HStack(spacing: 7) {
                    Circle()
                        .fill(color(of: host))
                        .frame(width: 7, height: 7)
                    Text(host.name)
                        .font(.system(size: 12, weight: .medium))
                        .lineLimit(1)
                    Spacer(minLength: 4)
                    Text(detail(of: host))
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                        .monospacedDigit()
                }
                .help(host.error ?? detail(of: host))
            }
            Menu {
                Button("Settings…") { openSettings() }
                Button("Add a Host…") { store.showsAddHost = true }
                Divider()
                Button("Sign Out") { store.signOut() }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "person.crop.circle")
                    Text(store.account.email)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .animation(.easeOut(duration: 0.15), value: store.undo)
    }

    private func color(of host: Host) -> Color {
        switch host.state {
        case .connected: return Color.themeSuccess
        case .connecting: return Color.themeWarning
        case .disconnected, .refused: return Color.themeDanger
        }
    }

    private func detail(of host: Host) -> String {
        switch host.state {
        case .connected:
            let path = host.path ?? "connected"
            return host.rttMs.map { "\(path) · \($0) ms" } ?? path
        case .connecting: return "connecting…"
        case .disconnected: return "offline"
        case .refused: return "refused"
        }
    }
}
