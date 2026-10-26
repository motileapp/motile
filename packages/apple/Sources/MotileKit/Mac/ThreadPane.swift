#if os(macOS)
import AppKit
import SwiftUI

/// The right side of the window: the open thread with the composer over its end, or the start
/// of a new one.
struct ThreadPane: View {
    @Environment(AppStore.self) private var store
    /// How far the title starts from the pane's left edge: past the window's buttons when the
    /// sidebar is hidden.
    var titleInset: CGFloat = besideSidebarInset
    static let besideSidebarInset: CGFloat = 20
    /// The right panel is beside the pane, so the window's last button isn't over it.
    var besidePanel = false

    private static let composerBottomGap: CGFloat = 16

    private var isStart: Bool {
        guard let draft = store.selectedDraft else { return false }
        return store.transcriptIsEmpty && !store.isSending(draft)
    }

    var body: some View {
        @Bindable var store = store
        let agent = store.shownAgent
        ZStack(alignment: .bottom) {
            if isStart {
                start
            } else if let agent {
                AgentBar(agent: agent)
                    .padding(.horizontal, Theme.composerPadding)
                    .padding(.top, TranscriptView.composerGap)
                    .padding(.bottom, Self.composerBottomGap)
                    .frame(maxWidth: .infinity)
                    .transformAnchorPreference(key: ComposerPlace.self, value: .bounds) { $0.room = $1 }
            } else if store.openThreadRemovedServer != nil {
                RoomWithoutComposer(bottomGap: Self.composerBottomGap)
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
        .coordinateSpace(.named(ComposerView.roomSpace))
        .transcriptBehind(of: store, ofAgent: agent != nil, shown: !isStart, under: [])
        .takingDroppedFiles()
        .overlay(alignment: .topLeading) { topBar }
        .navigationTitle(store.selectedThread?.title ?? "New thread")
        .modal(item: $store.committingProject) { project in
            CommitSheet(project: project)
        }
        .overlay(alignment: .topTrailing) {
            if let notice = store.openNotice {
                NoticeView(notice: notice)
                    .padding(.top, 6)
                    .padding(.trailing, 14)
                    .appearing(.opacity.combined(with: .offset(y: -6)))
            }
        }
        .animation(.easeOut(duration: 0.15), value: store.notice)
    }

    /// The thread's project and name and its git button, drawn in the window's top bar over
    /// this pane.
    private var topBar: some View {
        GeometryReader { proxy in
            HStack(spacing: 8) {
                HStack(spacing: ControlSize.regular.padding) {
                    if store.shownAgent != nil {
                        backButton(help: "Back to the thread", bar: proxy.safeAreaInsets.top) { store.rightPanel.showAgents() }
                    } else if let parent = store.parentThread {
                        backButton(help: "Open \(parent.title)", bar: proxy.safeAreaInsets.top) { store.openStartedThread(parent.id) }
                    }
                    title
                }
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
            // Room for the button that shows the right panel, which is at the window's edge.
            .padding(.trailing, besidePanel ? 4 : ToolbarButton.width + 12)
            .frame(maxWidth: .infinity)
            .frame(height: proxy.safeAreaInsets.top)
            .offset(y: -proxy.safeAreaInsets.top)
        }
    }

    /// Before the title, the way back to the thread from the agent shown in its place, or to the
    /// thread whose agent started the open one. Its light is as far from the pane's side as from
    /// the bar's top, beside the sidebar.
    private func backButton(help: String, bar: CGFloat, action: @escaping () -> Void) -> some View {
        let size = ControlSize.large
        let light = (size.height - size.symbolSide) / 2
        return ActionButton(icon: .cornerLeftUp, help: help, size: size, action: action)
            .padding(.leading, (bar - size.height) / 2 - Self.besideSidebarInset)
            .padding(.trailing, -light)
    }

    /// The project, then the thread's name, or the agent's when one is shown in its place.
    private var title: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let parts = store.composerProjectLine {
                ProjectLine(project: store.composerProject, parts: parts, size: 11, iconSize: 12)
            }
            Text(store.shownAgent?.title ?? store.selectedThread?.title ?? "New thread")
                .font(.ui(size: 13, weight: .semibold))
                .foregroundStyle(Color.themeForeground)
        }
        .lineLimit(1)
        .allowsHitTesting(false)
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
                ForEach(store.pickableProjects) { project in
                    Button {
                        store.setNewThreadProject(project.id)
                    } label: {
                        Label {
                            Text(project.name)
                        } icon: {
                            Image(platform: project.menuIcon ?? .symbol(project.menuSymbol, size: 13))
                        }
                    }
                }
                Divider()
                Button("Add Project") { store.addProject() }
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
