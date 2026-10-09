#if os(macOS)
import AppKit
import SwiftUI

/// The right side of the window: the open thread with the composer over its end, or the start
/// of a new one.
struct ThreadPane: View {
    @Environment(AppStore.self) private var store
    /// How far the title starts from the pane's left edge: past the window's buttons when the
    /// sidebar is hidden.
    var titleInset: CGFloat = 20
    /// The side panel is beside the pane, so the window's last button isn't over it.
    var besidePanel = false
    @State private var removal: Removal?

    private static let composerBottomGap: CGFloat = 16

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
                ComposerView()
                    .padding(.horizontal, Theme.composerPadding)
                    .padding(.top, TranscriptView.composerGap)
                    .padding(.bottom, Self.composerBottomGap)
                    .frame(maxWidth: .infinity)
                    .transformAnchorPreference(key: ComposerPlace.self, value: .bounds) { $0.room = $1 }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
        .transcriptBehind(of: store, shown: !isStart, under: [])
        .takingDroppedFiles()
        .overlay(alignment: .topLeading) { topBar }
        .navigationTitle(store.selectedThread?.title ?? "New thread")
        .sheet(item: $store.committingProject) { project in
            CommitSheet(project: project)
                .sheetSurface()
        }
        .overlay(alignment: .topTrailing) {
            if let notice = store.openGitNotice {
                GitNoticeView(notice: notice)
                    .padding(.top, 6)
                    .padding(.trailing, 14)
                    .appearing(.opacity.combined(with: .offset(y: -6)))
            }
        }
        .animation(.easeOut(duration: 0.15), value: store.gitNotice)
        .confirmsRemoval($removal)
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
                } else if let project = store.gitProject, store.canInitializeGit(of: project) {
                    InitializeGitButton(project: project)
                } else if let base = store.draftBaseToPull {
                    PullBaseButton(base: base)
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
            if let parts = store.composerProjectLine {
                ProjectLine(project: store.composerProject, parts: parts, size: 11, iconSize: 12)
            }
            Text(store.selectedThread?.title ?? "New thread")
                .font(.ui(size: 13, weight: .semibold))
                .foregroundStyle(Color.themeForeground)
        }
        .lineLimit(1)
    }

    /// The empty state of a new thread: a question, and the composer a little above the middle
    /// of the window.
    private var start: some View {
        GeometryReader { window in
            startBlock
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(.bottom, window.size.height * Self.startLift * 2)
        }
        .ignoresSafeArea(edges: .top)
    }

    /// How far above the middle the start is, as a part of the window's height.
    private static let startLift: CGFloat = 0.03

    private var startBlock: some View {
        VStack(spacing: 26) {
            if store.projects.isEmpty && store.noProjects.isEmpty {
                VStack(spacing: 10) {
                    Text("Add a project to start")
                        .font(.ui(size: 28, weight: .regular))
                    Text("A project is a folder on your server that threads work in.")
                        .font(.ui(size: 14))
                        .foregroundStyle(Color.themeMutedForeground)
                    ActionButton("Add Project", icon: .folderPlus, variant: .primary, size: .large) { store.addProject() }
                    .disabled(!store.servers.contains { $0.state == .connected })
                    .padding(.top, 8)
                }
            } else {
                headline
                ComposerView()
                    .padding(.horizontal, Theme.composerPadding)
            }
        }
    }

    private var headline: some View {
        let selected = store.project(store.selectedDraft?.projectID)
        let headline = Project.headline(selected)
        return HStack(spacing: Self.headlineWordSpace) {
            Text(headline.lead)
                .foregroundStyle(Color.themeForeground)
            Menu {
                ForEach(store.recentProjects + store.noProjects) { project in
                    Button {
                        store.setNewThreadProject(project.id)
                    } label: {
                        let name = store.servers.count > 1 ? "\(project.name) · \(store.server(project.serverID)?.shortName ?? "")" : project.name
                        Label {
                            Text(name)
                        } icon: {
                            Image(platform: project.menuIcon ?? .symbol(project.menuSymbol, size: 13))
                        }
                    }
                }
                Divider()
                Button("Add Project") { store.addProject() }
                if let project = selected, !project.noProject {
                    Button("Choose an Icon for “\(project.name)”") { store.openPanel(.icon(project.id)) }
                    Button("Remove “\(project.name)” from Projects…") { removal = .project(project) }
                }
            } label: {
                HStack(spacing: 8) {
                    ProjectIcon(project: selected, size: 22)
                    Text(headline.name)
                    Image(.chevronDown, size: 13)
                        .foregroundStyle(Color.themeMutedStrongerForeground)
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
            .hoverHighlight(radius: Radius.lg)
            .overlay {
                RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).stroke(Color.themeBorder, lineWidth: 1)
            }
        }
        .font(.ui(size: Self.headlineSize, weight: .regular))
    }

    private static let headlineSize: CGFloat = 28
    private static let headlineWordSpace = (" " as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: headlineSize)]).width
}
#endif
