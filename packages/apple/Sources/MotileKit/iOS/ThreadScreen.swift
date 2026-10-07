#if os(iOS)
import SwiftUI

/// The open thread with the composer over its end, or the start of a new one, under a top bar
/// with the thread's name, its git button and the way to its files and changes.
struct ThreadScreen: View {
    @Environment(AppStore.self) private var store
    @Environment(Drawer.self) private var drawer
    /// The sidebar is under the thread, so the top bar has the button that shows it.
    var overSidebar = true
    @State private var composerHeight: CGFloat = 120

    private static let composerBottomGap: CGFloat = 8

    private var isStart: Bool {
        guard let draft = store.selectedDraft else { return false }
        return store.transcriptIsEmpty && !store.sendingDraftIDs.contains(draft.id)
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            if isStart {
                start
            }
            if !isStart || !store.projects.isEmpty {
                ComposerView()
                    .padding(.horizontal, Theme.composerPadding)
                    .padding(.top, TranscriptView.composerGap)
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
        .transcriptBehind(of: store, shown: !isStart, under: .vertical)
        .takingDroppedFiles()
        .background(Color.themeBackground.ignoresSafeArea())
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.hidden, for: .navigationBar)
        .toolbar { bar }
        .overlay(alignment: .topTrailing) {
            if let notice = store.openGitNotice {
                GitNoticeView(notice: notice)
                    .padding(.top, 6)
                    .padding(.horizontal, 14)
                    .appearing(.opacity.combined(with: .offset(y: -6)))
            }
        }
        .animation(.easeOut(duration: 0.15), value: store.gitNotice)
    }

    @ToolbarContentBuilder private var bar: some ToolbarContent {
        if overSidebar {
            ToolbarItem(placement: .topBarLeading) {
                Button {
                    drawer.isOpen.toggle()
                } label: {
                    menuIcon
                }
                .accessibilityLabel("Threads")
            }
        }
        ToolbarItem(placement: .principal) { title }
        ToolbarItemGroup(placement: .topBarTrailing) {
            if let project = store.gitProject, let control = project.gitControl {
                GitButton(project: project, control: control)
            } else if let project = store.gitProject, store.canInitializeGit(of: project) {
                InitializeGitButton(project: project)
            }
            Button {
                store.sidePanel.isOpen.toggle()
            } label: {
                Image(.panelRight, size: 16)
            }
            .accessibilityLabel("Files and changes")
        }
    }

    /// The colour of the first thread, other than the open one, that waits for the user or has a
    /// reply they haven't seen, or else the working colour while one works, or else the
    /// monitoring colour while one monitors.
    private var attention: Color? {
        let others = store.activeThreads.filter { store.selection != .thread($0.id) }
        if let color = others.lazy.compactMap(\.attentionColor).first { return color }
        if others.contains(where: { $0.running || $0.gitStage != nil }) { return .themeWorking }
        guard others.contains(where: \.monitoring) else { return nil }
        return .themeMonitoring
    }

    /// The menu icon, with a dot on its top right corner, cut out of the icon, when another
    /// thread needs attention, works or monitors, in the colour of its status.
    @ViewBuilder private var menuIcon: some View {
        let side = PlatformImage.symbolSide(16)
        let dot = CGPoint(x: side * 20 / 24, y: side * 5 / 24)
        if let attention {
            Image(.menu, size: 16)
                .mask {
                    Rectangle()
                        .overlay { Circle().frame(width: 12, height: 12).position(dot).blendMode(.destinationOut) }
                        .compositingGroup()
                }
                .overlay { Circle().fill(attention).frame(width: 8, height: 8).position(dot) }
        } else {
            Image(.menu, size: 16)
        }
    }

    private var title: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(store.selectedThread?.title ?? "New thread")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Color.themeText)
            if let parts = store.composerProjectLine {
                ProjectLine(project: store.composerProject, parts: parts, size: 12, iconSize: 12)
            }
        }
        .lineLimit(1)
        .frame(idealWidth: 10000, maxWidth: .infinity, alignment: .leading)
    }

    /// The empty state of a new thread: a question, over the composer.
    private var start: some View {
        VStack(spacing: 10) {
            if store.projects.isEmpty {
                Text("Add a project to start")
                    .font(.system(size: 26, weight: .regular))
                Text("A project is a folder on your server that threads work in.")
                    .font(.system(size: 16))
                    .foregroundStyle(Color.themeSecondary)
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
        .padding(.bottom, composerHeight)
        .contentShape(Rectangle())
        .onTapGesture { Platform.endEditing() }
    }

    private var headline: some View {
        let selected = store.project(store.selectedDraft?.projectID)
        return VStack(spacing: 12) {
            Text("Let’s build in")
                .foregroundStyle(Color.themeText)
            Menu {
                ForEach(store.recentProjects) { project in
                    Button {
                        store.setNewThreadProject(project.id)
                    } label: {
                        let name = store.servers.count > 1 ? "\(project.name) · \(store.server(project.serverID)?.shortName ?? "")" : project.name
                        Label {
                            Text(name)
                        } icon: {
                            Image(platform: project.menuIcon ?? .symbol(.folder, size: 15))
                        }
                    }
                }
                Divider()
                Button("Add Project…") { store.addProject() }
                if let project = selected {
                    Button("Choose an Icon for “\(project.name)”…") { store.iconProject = project }
                    Button("Use the Icon in Its Folder") { store.setIcon(of: project, to: nil) }
                    Button("Remove “\(project.name)” from Projects", role: .destructive) { store.removeProject(project) }
                }
            } label: {
                HStack(spacing: 9) {
                    ProjectIcon(project: selected, size: 24)
                    Text(selected?.name ?? "a project")
                        .foregroundStyle(Color.themeText)
                        .lineLimit(1)
                    Image(.chevronDown, size: 11)
                        .foregroundStyle(Color.themeTertiary)
                }
                .padding(.leading, 12)
                .padding(.trailing, 14)
                .frame(height: 48)
                .contentShape(Rectangle())
                .overlay {
                    RoundedRectangle(cornerRadius: 14, style: .continuous).stroke(Color.themeBorder, lineWidth: 1)
                }
            }
            .buttonStyle(.plain)
        }
        .font(.system(size: 26, weight: .regular))
    }
}
#endif
