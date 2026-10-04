#if os(macOS)
import AppKit
import SwiftUI

/// The right side of the window: the open thread with the composer over its end, or the start
/// of a new one.
struct ThreadPane: View {
    @Environment(AppStore.self) private var store
    @State private var composerHeight: CGFloat = 120
    /// How far the title starts from the pane's left edge: past the window's buttons when the
    /// sidebar is hidden.
    var titleInset: CGFloat = 20
    /// The side panel is beside the pane, so the window's last button isn't over it.
    var besidePanel = false

    private var isStart: Bool {
        guard let draft = store.selectedDraft else { return false }
        return store.transcriptIsEmpty && !store.sendingDraftIDs.contains(draft.id)
    }

    var body: some View {
        @Bindable var store = store
        ZStack(alignment: .bottom) {
            if isStart {
                start
            } else {
                TranscriptRepresentable(store: store, bottomInset: composerHeight)
                ComposerView()
                    .padding(.horizontal, Theme.contentPadding)
                    .padding(.top, TranscriptView.composerGap)
                    .padding(.bottom, 16)
                    .frame(maxWidth: .infinity)
                    .onGeometryChange(for: CGFloat.self) { proxy in
                        proxy.size.height
                    } action: { height in
                        composerHeight = height
                    }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay(alignment: .topLeading) { topBar }
        .navigationTitle(store.selectedThread?.title ?? "New thread")
        .sheet(item: $store.committingProject) { project in
            CommitSheet(project: project)
        }
        .overlay(alignment: .topTrailing) {
            if let notice = store.gitNotice, notice.projectID == store.gitProject?.id {
                GitNoticeView(notice: notice)
                    .padding(.top, 6)
                    .padding(.trailing, 14)
                    .transition(.opacity)
            }
        }
        .animation(.easeOut(duration: 0.15), value: store.gitNotice)
        .sheet(item: $store.iconProject) { project in
            if let server = store.server(project.serverID) {
                FolderPicker(server: server, iconFor: project)
            }
        }
    }

    /// The thread's project and name and its git button, drawn in the window's top bar over
    /// this pane.
    private var topBar: some View {
        GeometryReader { proxy in
            HStack(spacing: 8) {
                title
                    .allowsHitTesting(false)
                Spacer(minLength: 0)
                if let project = store.gitProject, let control = project.gitControl {
                    GitButton(project: project, control: control)
                }
            }
            .padding(.leading, titleInset)
            // Room for the button that shows the side panel, which is at the window's edge.
            .padding(.trailing, besidePanel ? 4 : ToolbarButton.width + 12)
            .frame(maxWidth: .infinity)
            .frame(height: proxy.safeAreaInsets.top)
            .offset(y: -proxy.safeAreaInsets.top)
        }
    }

    private var title: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let projectLine {
                HStack(spacing: 6) {
                    ProjectIcon(project: titleProject, size: 14)
                    Text(projectLine)
                        .font(.ui(size: 11, weight: .medium))
                        .foregroundStyle(Color.themeSecondary)
                }
            }
            Text(store.selectedThread?.title ?? "New thread")
                .font(.ui(size: 13, weight: .semibold))
                .foregroundStyle(Color.themeText)
        }
        .lineLimit(1)
    }

    private var titleProject: Project? {
        store.composerProject
    }

    private var projectLine: String? {
        let project = titleProject
        let folder = store.selectedThread.map { URL(fileURLWithPath: $0.cwd).lastPathComponent }
        guard let name = project?.name ?? folder else { return nil }
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
                        .font(.ui(size: 28, weight: .regular))
                    Text("A project is a folder on your server that threads work in.")
                        .font(.ui(size: 14))
                        .foregroundStyle(Color.themeSecondary)
                    Button {
                        store.addProject()
                    } label: {
                        Label("Add Project", systemImage: "folder.badge.plus")
                    }
                    .controlSize(.large)
                    .disabled(!store.servers.contains { $0.state == .connected })
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
        let selected = store.project(store.selectedDraft?.projectID)
        return HStack(spacing: Self.headlineWordSpace) {
            Text("Let’s build in")
                .foregroundStyle(Color.themeSecondary)
            Menu {
                ForEach(store.recentProjects) { project in
                    Button {
                        store.setNewThreadProject(project.id)
                    } label: {
                        let name = store.servers.count > 1 ? "\(project.name) · \(store.server(project.serverID)?.name ?? "")" : project.name
                        Label {
                            Text(name)
                        } icon: {
                            Image(platform: project.menuIcon ?? NSImage(systemSymbolName: "folder", accessibilityDescription: nil) ?? NSImage())
                        }
                    }
                }
                Divider()
                Button("Add Project…") { store.addProject() }
                if let project = selected {
                    Button("Choose an Icon for “\(project.name)”…") { store.iconProject = project }
                    Button("Use the Icon in Its Folder") { store.setIcon(of: project, to: nil) }
                    Button("Remove “\(project.name)” from Projects") { store.removeProject(project) }
                }
            } label: {
                HStack(spacing: 8) {
                    ProjectIcon(project: selected, size: 22)
                    Text(selected?.name ?? "a project")
                    Image(systemName: "chevron.down")
                        .font(.ui(size: 13, weight: .bold))
                        .foregroundStyle(Color.themeTertiary)
                }
                .padding(.leading, 9)
                .padding(.trailing, 11)
                .padding(.vertical, 3)
                .contentShape(Rectangle())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .hoverHighlight(radius: 10)
            .overlay {
                RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
            }
        }
        .font(.ui(size: Self.headlineSize, weight: .regular))
    }

    private static let headlineSize: CGFloat = 28
    private static let headlineWordSpace = (" " as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: headlineSize)]).width
}
#endif
