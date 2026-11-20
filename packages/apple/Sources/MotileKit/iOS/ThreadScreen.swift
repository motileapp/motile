#if os(iOS)
import SwiftUI

extension EnvironmentValues {
    /// Whether the screen's keyboard is up under the view.
    @Entry var keyboardShown = false
}

/// The open thread with the composer over its end, or the start of a new one, under a top bar
/// with the thread's name, its git button and the way to its files and changes.
struct ThreadScreen: View {
    @Environment(AppStore.self) private var store
    @Environment(Drawer.self) private var drawer
    /// The sidebar is under the thread, so the top bar has the button that shows it.
    var overSidebar = true
    @State private var composerHeight: CGFloat = 120
    @State private var renaming: ThreadInfo?
    @State private var deleting: ThreadInfo?
    @State private var leadingEdge: CGFloat?
    @State private var trailingEdge: CGFloat?
    @State private var screenFrame = CGRect.zero
    /// Where the bar put the title, which it places as it likes.
    @State private var titleMinX: CGFloat?

    private static let composerBottomGap: CGFloat = 8
    /// The room between the title and the bar's buttons.
    private static let titleGap: CGFloat = 12

    /// Whether the keyboard takes the bottom of the window under `frame`. Read while the view is
    /// laid out, so that what changes with it changes in the keyboard's own animation.
    private static func keyboardShown(under frame: CGRect) -> Bool {
        let window = UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first
        guard let window else { return false }
        return window.bounds.maxY - frame.maxY > window.safeAreaInsets.bottom + 1
    }

    private var isStart: Bool {
        guard let draft = store.selectedDraft else { return false }
        return store.transcriptIsEmpty && !store.isSending(draft)
    }

    var body: some View {
        let agent = store.shownAgent
        GeometryReader { proxy in
            bottom(agent: agent)
                .environment(\.keyboardShown, Self.keyboardShown(under: proxy.frame(in: .global)))
        }
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { screenFrame = $0 }
        .coordinateSpace(.named(ComposerView.roomSpace))
        .transcriptBehind(of: store, ofAgent: agent != nil, shown: !isStart, under: .vertical)
        .takingDroppedFiles()
        .background(Color.themeBackground.ignoresSafeArea())
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        .toolbar { bar }
        .overlay(alignment: .topTrailing) { ThreadCorner() }
        .threadPrompts(renaming: $renaming, deleting: $deleting)
    }

    /// The start of a new thread, and the composer or the bar that takes its place.
    private func bottom(agent: AgentInfo?) -> some View {
        ZStack(alignment: .bottom) {
            if isStart {
                start
            }
            if let agent, !isStart {
                AgentBar(agent: agent)
                    .padding(.horizontal, Theme.composerPadding)
                    .padding(.top, TranscriptView.overlayGap)
                    .padding(.bottom, Self.composerBottomGap)
                    .frame(maxWidth: .infinity)
                    .transformAnchorPreference(key: ComposerPlace.self, value: .bounds) { $0.room = $1 }
            } else if let server = store.openThreadRemovedServer {
                UnlinkedThreadBar(server: server)
                    .padding(.horizontal, Theme.composerPadding)
                    .padding(.top, TranscriptView.overlayGap)
                    .padding(.bottom, Self.composerBottomGap)
                    .frame(maxWidth: .infinity)
                    .transformAnchorPreference(key: ComposerPlace.self, value: .bounds) { $0.room = $1 }
            } else if !isStart || !store.projects.isEmpty || !store.noProjects.isEmpty {
                ComposerView()
                    .padding(.horizontal, Theme.composerPadding)
                    .padding(.top, TranscriptView.overlayGap)
                    .padding(.bottom, Self.composerBottomGap)
                    .frame(maxWidth: .infinity)
                    .onGeometryChange(for: CGFloat.self) { proxy in
                        proxy.size.height
                    } action: { height in
                        composerHeight = height
                    }
                    .transformAnchorPreference(key: ComposerPlace.self, value: .bounds) { $0.room = $1 }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
    }

    private var hasLeading: Bool {
        overSidebar || store.shownAgent != nil || store.parentThread != nil
    }

    @ToolbarContentBuilder private var bar: some ToolbarContent {
        if hasLeading {
            GlassToolbarItem(placement: .topBarLeading) {
                HStack(spacing: 0) { leading }
                    .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).maxX } action: { leadingEdge = $0 }
            }
        }
        ToolbarItem(placement: .principal) { titleMenu }
            .withoutSystemGlass()
        GlassToolbarItem(placement: .topBarTrailing) {
            HStack(spacing: 0) { trailing }
                .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).minX } action: { trailingEdge = $0 }
        }
    }

    @ViewBuilder private var leading: some View {
        if overSidebar {
            Button {
                drawer.isOpen.toggle()
            } label: {
                menuIcon
            }
            .accessibilityLabel("Threads")
        }
        if store.shownAgent != nil {
            Button {
                store.rightPanel.showAgents()
            } label: {
                Image(.cornerLeftUp, size: .lg)
            }
            .accessibilityLabel("Back to the thread")
        } else if let parent = store.parentThread {
            Button {
                store.openStartedThread(parent.id)
            } label: {
                Image(.cornerLeftUp, size: .lg)
            }
            .accessibilityLabel("Open \(parent.title)")
        }
    }

    @ViewBuilder private var trailing: some View {
        if let project = store.gitProject, let control = project.gitControl {
            GitButton(project: project, control: control).glassSymbolFrame()
        } else if let project = store.gitProject, store.canInitializeGit(of: project) {
            InitializeGitButton(project: project).glassSymbolFrame()
        } else if let base = store.draftBaseToPull {
            PullBaseButton(base: base).glassSymbolFrame()
        }
        Button {
            store.rightPanel.isOpen.toggle()
        } label: {
            Image(.panelRight, size: .lg)
        }
        .accessibilityLabel("Files and changes")
    }

    /// The colour of the first thread, other than the open one, that waits for the user or has a
    /// reply they haven't seen, or else the working colour while one works, or else the
    /// monitoring colour while one monitors.
    private var attention: Color? {
        let others = store.listedThreads.filter { store.selection != .thread($0.id) }
        if let color = others.lazy.compactMap(\.attentionColor).first { return color }
        if others.contains(where: { $0.working || $0.gitStage != nil }) { return .themeProcess }
        guard others.contains(where: \.monitoring) else { return nil }
        return .themeForeground
    }

    /// The menu icon, with a dot on its top right corner, cut out of the icon, when another
    /// thread needs attention, works or monitors, in the colour of its status.
    @ViewBuilder private var menuIcon: some View {
        let side = PlatformImage.symbolSide(.lg)
        let dot = CGPoint(x: side * 20 / 24, y: side * 5 / 24)
        if let attention {
            Image(.menu, size: .lg)
                .mask {
                    Rectangle()
                        .overlay { Circle().frame(width: 12, height: 12).position(dot).blendMode(.destinationOut) }
                        .compositingGroup()
                }
                .overlay { Circle().fill(attention).frame(width: 8, height: 8).position(dot) }
        } else {
            Image(.menu, size: .lg)
        }
    }

    /// The title opens what can be done with the thread, all of it as one button that dims under a
    /// finger, as the bar has no room around it to light.
    @ViewBuilder private var titleMenu: some View {
        let groups = store.titleActions(rename: { renaming = $0 }, delete: { deleting = $0 })
        let room = titleRoom
        let label = title.frame(width: room.map { $0.end - $0.start }, alignment: .leading)
        Group {
            if groups.isEmpty {
                label
            } else {
                Menu {
                    RowActionItems(groups: groups)
                } label: {
                    label.contentShape(Rectangle())
                }
                .buttonStyle(DimButtonStyle())
                .accessibilityLabel("Thread menu")
            }
        }
        .padding(.leading, max(0, (room?.start ?? 0) - (titleMinX ?? 0)))
        .frame(width: room.map { 2 * max(screenFrame.midX - $0.start, $0.end - screenFrame.midX) }, alignment: .leading)
        .onGeometryChange(for: CGFloat.self) { $0.frame(in: .global).minX } action: { titleMinX = $0 }
    }

    /// The room between the bar's buttons, or the screen's edges where there are none. The bar
    /// sizes a menu's label to all of it and places the title as it likes, over its buttons too, so
    /// the title reaches as far both ways from the screen's middle and starts after the buttons.
    private var titleRoom: (start: CGFloat, end: CGFloat)? {
        guard screenFrame != .zero else { return nil }
        let leadingEdge = hasLeading ? leadingEdge : nil
        let start = leadingEdge.map { $0 + Self.titleGap } ?? screenFrame.minX + Theme.contentPadding
        let end = trailingEdge.map { $0 - Self.titleGap } ?? screenFrame.maxX - Theme.contentPadding
        return (start, max(start, end))
    }

    /// The project, then the thread's name, or the agent's when one is shown in its place.
    private var title: some View {
        VStack(alignment: .leading, spacing: 1) {
            if let parts = store.composerProjectLine {
                ProjectLine(project: store.composerProject, parts: parts, size: .xxs)
            }
            Text(store.shownAgent?.title ?? store.selectedThread?.title ?? "New thread")
                .font(.ui(.sm, weight: .semibold))
                .foregroundStyle(Color.themeForeground)
        }
        .lineLimit(1)
    }

    /// The empty state of a new thread: a question, over the composer.
    private var start: some View {
        VStack(spacing: 10) {
            if store.projects.isEmpty && store.noProjects.isEmpty {
                Text("Add a project to start")
                    .font(.ui(.xl))
                Text("A project is a folder on your server that threads work in.")
                    .font(.ui(.md))
                    .foregroundStyle(Color.themeMutedForeground)
                    .multilineTextAlignment(.center)
                ActionButton("Add Project", icon: .folderPlus, variant: .primary, size: .large) { store.addProject() }
                .disabled(!store.servers.contains { $0.state == .connected })
                .padding(.top, 10)
            } else {
                headline
            }
        }
        .padding(.horizontal, 24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.bottom, composerHeight + TranscriptView.composerGap - TranscriptView.overlayGap)
        .contentShape(Rectangle())
        .onTapGesture { Platform.endEditing() }
    }

    private var headline: some View {
        let selected = store.project(store.selectedDraft?.projectID)
        let headline = Project.headline(selected)
        return VStack(spacing: 12) {
            Text(headline.lead)
                .foregroundStyle(Color.themeForeground)
            Button {
                store.openPanel(.draftProject)
            } label: {
                HStack(spacing: 9) {
                    ProjectIcon(project: selected, size: .xxl)
                    Text(headline.name)
                        .foregroundStyle(Color.themeForeground)
                        .lineLimit(1)
                    Image(.chevronDown, size: .md)
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                }
                .padding(.leading, 12)
                .padding(.trailing, 14)
                .frame(height: 48)
                .contentShape(Rectangle())
                .overlay {
                    RoundedRectangle(cornerRadius: Radius.lg, style: .continuous).stroke(Color.themeBorder, lineWidth: 1)
                }
            }
            .buttonStyle(.plain)
        }
        .font(.ui(.xl))
    }
}
#endif
