import SwiftUI

/// The right side of the window: the open thread with the composer over its end, or the start
/// of a new one.
struct ThreadPane: View {
    @Environment(AppStore.self) private var store
    @State private var composerHeight: CGFloat = 120

    private var isStart: Bool {
        store.selection == .newThread && store.transcriptIsEmpty && !store.sending
    }

    var body: some View {
        @Bindable var store = store
        ZStack(alignment: .bottom) {
            Color.themeBackground.ignoresSafeArea()
            if isStart {
                start
            } else {
                TranscriptRepresentable(store: store, bottomInset: composerHeight + 8)
                if store.transcriptIsEmpty && !store.activity.running {
                    Text("Send a message to start the conversation.")
                        .font(.system(size: 13))
                        .foregroundStyle(Color.themeTertiary)
                        .frame(maxHeight: .infinity)
                }
                ComposerView()
                    .padding(.horizontal, Theme.contentPadding)
                    .padding(.top, 24)
                    .padding(.bottom, 16)
                    .frame(maxWidth: .infinity)
                    .background(composerBackdrop)
                    .background(
                        GeometryReader { proxy in
                            Color.clear.preference(key: ComposerHeightKey.self, value: proxy.size.height)
                        }
                    )
            }
        }
        .onPreferenceChange(ComposerHeightKey.self) { composerHeight = $0 }
        .navigationTitle(store.selectedThread?.title ?? "New thread")
        .navigationSubtitle(subtitle)
        .toolbar {
            ToolbarItemGroup {
                if let thread = store.selectedThread {
                    Button {
                        store.toggleDone()
                    } label: {
                        Label(thread.isDone ? "Mark Undone" : "Mark Done", systemImage: thread.isDone ? "arrow.uturn.backward.circle" : "checkmark.circle")
                    }
                    .help(thread.isDone ? "Mark undone (⇧⌘D)" : "Mark done (⇧⌘D)")
                    .disabled(thread.running)
                }
            }
        }
        .sheet(isPresented: $store.showsFolderPicker) {
            if let host = store.composerHost {
                FolderPicker(host: host)
            }
        }
    }

    /// Fades the transcript out under the composer, so nothing shows through around it.
    private var composerBackdrop: some View {
        LinearGradient(
            stops: [
                .init(color: Color.themeBackground.opacity(0), location: 0),
                .init(color: Color.themeBackground, location: 0.45),
            ],
            startPoint: .top,
            endPoint: .bottom
        )
        .allowsHitTesting(false)
    }

    private var subtitle: String {
        guard let thread = store.selectedThread else { return "" }
        let project = store.project(thread.projectID)
        let name = project?.name ?? URL(fileURLWithPath: thread.cwd).lastPathComponent
        guard let branch = project?.branch else { return name }
        return "\(name) · \(branch)"
    }

    /// The empty state of a new thread: a question, and the composer in the middle of the pane.
    private var start: some View {
        VStack(spacing: 26) {
            Spacer()
            if store.projects.isEmpty {
                VStack(spacing: 10) {
                    Text("Add a project to start")
                        .font(.system(size: 28, weight: .regular))
                    Text("A project is a folder on your host that threads work in.")
                        .font(.system(size: 14))
                        .foregroundStyle(Color.themeSecondary)
                    Button {
                        store.showsFolderPicker = true
                    } label: {
                        Label("Add Project", systemImage: "folder.badge.plus")
                    }
                    .controlSize(.large)
                    .disabled(store.composerHost?.state != .connected)
                    .padding(.top, 8)
                }
            } else {
                headline
                ComposerView()
                    .padding(.horizontal, Theme.contentPadding)
            }
            Spacer()
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var headline: some View {
        HStack(spacing: 8) {
            Text("What should we build in")
            Menu {
                ForEach(store.projects) { project in
                    Button {
                        store.setNewThreadProject(project.id)
                    } label: {
                        Text(store.hosts.count > 1 ? "\(project.name) · \(store.host(project.hostID)?.name ?? "")" : project.name)
                    }
                }
                Divider()
                Button("Add Project…") { store.showsFolderPicker = true }
                if let project = store.project(store.newThread.projectID) {
                    Button("Remove “\(project.name)” from Projects") { store.removeProject(project) }
                }
            } label: {
                HStack(spacing: 4) {
                    Text(store.project(store.newThread.projectID)?.name ?? "a project")
                    Image(systemName: "chevron.down")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Color.themeTertiary)
                }
                .foregroundStyle(Color.themePrimary)
            }
            .menuStyle(.button)
        .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            Text("?")
                .padding(.leading, -6)
        }
        .font(.system(size: 28, weight: .regular))
    }
}

private struct ComposerHeightKey: PreferenceKey {
    static var defaultValue: CGFloat = 120
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}
