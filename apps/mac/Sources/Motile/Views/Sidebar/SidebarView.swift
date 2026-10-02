import SwiftUI

/// Every active thread on every host in one list, with the ones marked done on a shelf at the
/// bottom.
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
        ScrollView {
            LazyVStack(spacing: 2) {
                ForEach(active) { thread in
                    ThreadRow(thread: thread, rename: beginRename, delete: { deleting = $0 })
                }
                if active.isEmpty {
                    Text(!search.isEmpty ? "No threads found" : done.isEmpty ? "No threads yet" : "No active threads")
                        .font(.system(size: 12))
                        .foregroundStyle(Color.themeTertiary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 6)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 4)
        }
        .searchable(text: $search, placement: .sidebar, prompt: "Search")
        .safeAreaInset(edge: .bottom, spacing: 0) {
            VStack(spacing: 0) {
                if !done.isEmpty {
                    DoneShelf(
                        threads: done,
                        expanded: search.isEmpty ? $doneExpanded : .constant(true),
                        limit: $doneLimit,
                        rename: beginRename,
                        delete: { deleting = $0 }
                    )
                }
                SidebarFooter()
            }
        }
        .background(GlassBackground())
        .toolbar {
            ToolbarItemGroup {
                Button {
                    store.showsFolderPicker = true
                } label: {
                    Label("Add Project", systemImage: "folder.badge.plus")
                }
                .help("Add a project")
                Button {
                    store.startNewThread()
                } label: {
                    Label("New Thread", systemImage: "square.and.pencil")
                }
                .help("New thread")
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
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                ProjectIcon(project: store.project(thread.projectID), size: 14)
                Text(projectName)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
                Spacer(minLength: 6)
                if hovering && !thread.running {
                    IconOnlyButton(symbol: "checkmark", help: "Mark done", size: 22, symbolSize: 12) {
                        store.setDone([thread.id], done: true, fromSidebar: true)
                    }
                } else {
                    ThreadStatus(thread: thread)
                }
            }
            .foregroundStyle(.secondary)
            .frame(height: 22)

            Text(thread.title)
                .font(.system(size: 13, weight: .medium))
                .lineLimit(1)
        }
        .padding(.horizontal, 8)
        .padding(.top, 3)
        .padding(.bottom, 7)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .hoverHighlight(radius: 8, selected: store.selection == .thread(thread.id))
        .onHover { hovering = $0 }
        .onTapGesture { store.select(.thread(thread.id)) }
        .contextMenu { ThreadMenu(thread: thread, rename: rename, delete: delete) }
    }

    private var projectName: String {
        let project = store.project(thread.projectID)?.name ?? URL(fileURLWithPath: thread.cwd).lastPathComponent
        guard store.hosts.count > 1, let host = store.host(thread.hostID) else { return project }
        return "\(project) · \(host.name)"
    }
}

/// The threads marked done, at the bottom of the sidebar: a line that opens into their list.
private struct DoneShelf: View {
    private static let rowHeight: CGFloat = 30
    private static let maxHeight: CGFloat = 250

    let threads: [ThreadInfo]
    @Binding var expanded: Bool
    @Binding var limit: Int
    let rename: (ThreadInfo) -> Void
    let delete: (ThreadInfo) -> Void

    var body: some View {
        let shown = Array(threads.prefix(limit))
        let more = threads.count - shown.count
        VStack(spacing: 0) {
            Divider()
            Button {
                expanded.toggle()
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .semibold))
                        .rotationEffect(.degrees(expanded ? 90 : 0))
                        .frame(width: 14)
                    Text("Done")
                        .font(.system(size: 12, weight: .medium))
                    Spacer()
                    Text("\(threads.count)")
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                        .monospacedDigit()
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .frame(height: Self.rowHeight)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .hoverHighlight(radius: 8)
            .padding(.horizontal, 10)
            .padding(.vertical, 4)

            if expanded {
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(shown) { thread in
                            DoneRow(thread: thread, rename: rename, delete: delete)
                                .frame(height: Self.rowHeight)
                        }
                        if more > 0 {
                            Button("Show \(min(more, 25)) more") { limit += 25 }
                                .buttonStyle(.plain)
                                .font(.system(size: 12))
                                .foregroundStyle(Color.themeSecondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 8)
                                .frame(height: Self.rowHeight)
                        }
                    }
                    .padding(.horizontal, 10)
                    .padding(.bottom, 4)
                }
                .frame(height: min(Self.maxHeight, CGFloat(shown.count + (more > 0 ? 1 : 0)) * (Self.rowHeight + 2) + 4))
            }
        }
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
                IconOnlyButton(symbol: "arrow.uturn.backward", help: "Mark undone", size: 22, symbolSize: 12) {
                    store.setDone([thread.id], done: false)
                }
                .foregroundStyle(.secondary)
            } else {
                TimelineView(.periodic(from: .now, by: 30)) { context in
                    Text(Time.ago(thread.doneAt ?? thread.updatedAt, now: context.date.timeIntervalSince1970))
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .padding(.horizontal, 8)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .hoverHighlight(radius: 8, selected: store.selection == .thread(thread.id))
        .onHover { hovering = $0 }
        .onTapGesture { store.select(.thread(thread.id)) }
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
                .font(.system(size: 11, weight: .semibold))
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
                Button("Add a Project…") { store.showsFolderPicker = true }
                Button("Add a Host…") { store.showsAddHost = true }
                Divider()
                Button("Sign Out") { store.signOut() }
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "person.crop.circle")
                        .font(.system(size: 14))
                    Text(store.account.email)
                        .font(.system(size: 12))
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer(minLength: 0)
                }
                .foregroundStyle(.secondary)
                .padding(.horizontal, 8)
                .frame(height: 30)
                .contentShape(Rectangle())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .hoverHighlight(radius: 8)
            .padding(.horizontal, -8)
        }
        .padding(.horizontal, 18)
        .padding(.top, 10)
        .padding(.bottom, 8)
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
