import SwiftUI

/// The git button in a thread's top bar. On the Mac its left half does the one thing the
/// repository calls for, at once: commit, push, open a pull request, pull. Its right half opens
/// the menu of all of them, where an item that can't run now says why. While an action runs, the
/// button says which stage it is at. On iOS it is one symbol that opens the menu: the one of the
/// thing the repository calls for, or of its pull request in the colour of its state.
struct GitButton: View {
    @Environment(AppStore.self) private var store
    let project: Project
    let control: GitControl

    #if os(macOS)
    private static let height = ControlSize.regular.height
    private static let radius = ControlSize.regular.radius
    /// What leaves the room under the button that the composer's menus leave over theirs.
    private static let menuGap: CGFloat = 16
    @State private var anchor = MenuAnchorView()
    #endif

    var body: some View {
        content
            .asking(Binding { store.pendingGit } set: { store.pendingGit = $0 }) { pending in
                Confirmation(
                    title: pending.confirm.title, message: pending.confirm.description,
                    answers: [
                        Answer(title: pending.confirm.branchOff, variant: .secondary) { store.confirmGit(pending, onNewBranch: true) },
                        Answer(title: pending.confirm.proceed) { store.confirmGit(pending, onNewBranch: false) },
                    ]
                )
            }
    }

    private func symbol(of quick: GitQuick) -> Symbol {
        guard quick.url == nil else { return quick.pullRequest?.state.symbol ?? .gitPullRequest }
        return GitSymbol.symbol(for: quick.action)
    }

    private func pullRequestColor(of quick: GitQuick) -> PlatformColor? {
        guard quick.url != nil, let state = quick.pullRequest?.state else { return nil }
        return state == .draft ? Theme.foreground : state.platformColor
    }

    #if os(macOS)
    private var content: some View {
        let stage = store.gitStage(in: project)
        let quick = control.quick
        return HStack(spacing: 0) {
            ActionButton(
                stage?.label ?? quick.title, icon: symbol(of: quick), help: quick.hint ?? quick.pullRequest?.title ?? quick.title, variant: .ghost,
                pending: stage != nil, joined: .all, tint: tint(of: quick, at: stage)
            ) {
                store.runQuickGit(in: project)
            }
            Rectangle()
                .fill(Color.themeBorder)
                .frame(width: 1, height: Self.height)
            menuButton
        }
        .clipShape(RoundedRectangle(cornerRadius: Self.radius, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).stroke(Color.themeBorder, lineWidth: 1)
        }
        .background(MenuAnchor(anchor: anchor))
        .padding(.horizontal, 6)
    }

    /// A pull request is in the colour of its state, and what has nothing to do is quiet.
    private func tint(of quick: GitQuick, at stage: GitStage?) -> Color? {
        if stage != nil { return .themeForeground }
        if let color = pullRequestColor(of: quick) { return Color(platform: color) }
        guard quick.action != nil || quick.url != nil else { return .themeMutedStrongerForeground }
        return .themeForeground
    }

    private var menuButton: some View {
        ActionButton.chevron(help: "Commit, push or open a pull request", joined: .all, action: showMenu)
    }

    /// Opens the menu under the button, its right edge on the button's. An item that can't run
    /// now is greyed, and says why when the pointer rests on it.
    private func showMenu() {
        guard let view = anchor.view else { return }
        let menu = NSMenu()
        menu.autoenablesItems = false
        let running = store.gitStage(in: project) != nil
        for item in control.menu {
            let entry = ActionMenuItem(title: item.label) { store.chooseGit(item, in: project) }
            entry.image = .symbol(GitSymbol.symbol(for: item.action), size: .md)
            entry.isEnabled = item.reason == nil && !running
            entry.toolTip = item.reason
            menu.addItem(entry)
        }
        if let warning = control.warning {
            menu.addItem(.separator())
            let note = NSMenuItem(title: warning, action: nil, keyEquivalent: "")
            note.isEnabled = false
            menu.addItem(note)
        }
        let below = view.isFlipped ? view.bounds.maxY + Self.menuGap : -Self.menuGap
        menu.popUp(positioning: nil, at: NSPoint(x: view.bounds.maxX - menu.size.width, y: below), in: view)
    }
    #else
    /// The menu of every action, under the one the repository calls for. One that can't run now
    /// is greyed, with why under its name.
    private var content: some View {
        let stage = store.gitStage(in: project)
        let running = stage != nil
        let quick = control.quick
        let showsQuick = running || ((quick.action != nil || quick.url != nil) && !control.menu.contains { $0.label == quick.label })
        return Menu {
            if showsQuick {
                Section {
                    Button(action: runQuick) {
                        quickLabel(quick, at: stage)
                    }
                }
            }
            ForEach(control.menu) { item in
                Button {
                    store.chooseGit(item, in: project)
                } label: {
                    Label(item.label, symbol: GitSymbol.symbol(for: item.action), size: .lg)
                    if let reason = item.reason { Text(reason) }
                }
                .disabled(item.reason != nil || running)
            }
            if let warning = control.warning {
                Section { note(warning) }
            }
        } label: {
            if running {
                Spinner(size: .lg)
            } else if let color = pullRequestColor(of: quick) {
                Image(symbol(of: quick), size: .lg).foregroundStyle(Color(platform: color))
            } else if quick.action != nil || quick.url != nil {
                Image(symbol(of: quick), size: .lg)
            } else {
                Image(.gitBranch, size: .lg)
            }
        }
        .accessibilityLabel("Git")
    }

    /// A line that only says something. A menu leaves out a section with nothing in it.
    private func note(_ text: String) -> some View {
        Button(text) {}.disabled(true)
    }

    /// What the left half of the Mac's button says: the stage of a run, or the action, or the pull
    /// request with its symbol in the colour of its state, which a menu only keeps drawn into the image.
    @ViewBuilder
    private func quickLabel(_ quick: GitQuick, at stage: GitStage?) -> some View {
        if let stage {
            Label(stage.label, symbol: .hourglass, size: .lg)
        } else if let color = pullRequestColor(of: quick) {
            Label {
                Text(quick.title)
            } icon: {
                Image(uiImage: TintedSymbol.image(symbol(of: quick), size: .lg, color: color))
            }
        } else {
            Label(quick.title, symbol: symbol(of: quick), size: .lg)
        }
    }

    private func runQuick() {
        guard store.gitStage(in: project) == nil else { return }
        store.runQuickGit(in: project)
    }
    #endif
}

#if os(macOS)
/// Where the git button is in its window, for its menu to open against.
private final class MenuAnchorView {
    weak var view: NSView?
}

private struct MenuAnchor: NSViewRepresentable {
    let anchor: MenuAnchorView

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        anchor.view = view
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        anchor.view = view
    }
}

/// A menu item that runs a closure.
private final class ActionMenuItem: NSMenuItem {
    private let run: () -> Void

    init(title: String, run: @escaping () -> Void) {
        self.run = run
        super.init(title: title, action: #selector(chosen), keyEquivalent: "")
        target = self
    }

    required init(coder: NSCoder) {
        fatalError("not used")
    }

    @objc private func chosen() {
        run()
    }
}
#endif

/// What stands in for the git button in a folder that is no git repository: it makes it one.
struct InitializeGitButton: View {
    @Environment(AppStore.self) private var store
    let project: Project

    var body: some View {
        let pending = store.initializingGit.contains(project.id)
        #if os(macOS)
        ActionButton("Initialize git", icon: .gitBranch, help: "Make \(project.name) a git repository", variant: .ghost, pending: pending) {
            store.initializeGit(in: project)
        }
        .padding(.horizontal, 6)
        #else
        Menu {
            Button {
                store.initializeGit(in: project)
            } label: {
                Label("Initialize git", symbol: .gitBranch, size: .lg)
            }
            .disabled(pending)
        } label: {
            if pending {
                Spinner(size: .lg)
            } else {
                Image(.gitBranch, size: .lg)
            }
        }
        .accessibilityLabel("Git")
        #endif
    }
}

/// What stands in for the git button in a draft that starts a new worktree, while the remote's
/// base has commits the local one lacks: it brings the local one up to date.
struct PullBaseButton: View {
    @Environment(AppStore.self) private var store
    let base: String

    var body: some View {
        let pending = store.pullingDraftBase
        let symbol = GitSymbol.symbol(for: "pull")
        #if os(macOS)
        ActionButton(
            "Pull \(base)", icon: symbol, help: "Bring \(base) up to date with \(store.draftStart ?? base)", variant: .ghost,
            pending: pending
        ) {
            store.pullDraftBase()
        }
        .padding(.horizontal, 6)
        #else
        Menu {
            Button {
                store.pullDraftBase()
            } label: {
                Label("Pull \(base)", symbol: symbol, size: .lg)
            }
            .disabled(pending)
        } label: {
            if pending {
                Spinner(size: .lg)
            } else {
                Image(symbol, size: .lg)
            }
        }
        .accessibilityLabel("Git")
        #endif
    }
}

/// The sheet behind the menu's Commit: the files to commit, which can be left out one by one,
/// and a message that is written for the user when they leave it empty.
struct CommitSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let project: Project
    @State private var message = ""
    @State private var excluded: Set<String> = []
    @State private var editing = false

    private static let rowHeight: CGFloat = Platform.scale > 1 ? 34 : 24

    private var files: [ChangedFile] { store.gitFiles }
    private var included: [String] { files.map(\.path).filter { !excluded.contains($0) } }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Commit Changes")
                    .font(.ui(size: 15, weight: .semibold))
                Text("Review and confirm your commit. Leave the message empty to have one written.")
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedForeground)
            }
            HStack(spacing: 8) {
                Text("Branch")
                    .foregroundStyle(Color.themeMutedForeground)
                Text(project.git?.branch ?? "No branch")
                    .fontWeight(.medium)
                Spacer()
                if project.git?.isDefault == true {
                    Text("Default branch")
                        .foregroundStyle(Color.themeWarning)
                }
            }
            .font(.ui(size: 12.5))
            fileList
            VStack(alignment: .leading, spacing: 6) {
                Text("Commit message (optional)")
                    .font(.ui(size: 12.5, weight: .medium))
                TextArea("Leave empty to have one written", text: $message, lines: 4)
            }
            #if os(macOS)
            HStack(spacing: 8) {
                Spacer()
                ActionButton("Cancel") { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Commit on New Branch") { commit(onNewBranch: true) }
                    .disabled(included.isEmpty)
                ActionButton("Commit", variant: .primary) { commit(onNewBranch: false) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(included.isEmpty)
            }
            #else
            VStack(spacing: 8) {
                ActionButton("Commit", variant: .primary, size: .large, fills: true) { commit(onNewBranch: false) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(included.isEmpty)
                ActionButton("Commit on New Branch", size: .large, fills: true) { commit(onNewBranch: true) }
                    .disabled(included.isEmpty)
                ActionButton("Cancel", variant: .ghost, size: .large, fills: true) { close() }
            }
            #endif
        }
        .padding(20)
        #if os(macOS)
        .frame(width: 460)
        #else
        .frame(maxHeight: .infinity, alignment: .top)
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        #endif
    }

    private var fileList: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text("Files")
                    .font(.ui(size: 12.5, weight: .medium))
                if !excluded.isEmpty, !editing {
                    Text("\(included.count) of \(files.count)")
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeMutedForeground)
                }
                Spacer()
                HStack(spacing: 0) {
                    if editing {
                        ActionButton(excluded.isEmpty ? "Select None" : "Select All", variant: .link, size: .small) {
                            excluded = excluded.isEmpty ? Set(files.map(\.path)) : []
                        }
                    }
                    ActionButton(editing ? "Done" : "Edit", variant: .link, size: .small) { editing.toggle() }
                }
                .padding(.vertical, -4)
                .padding(.trailing, -ControlSize.small.padding)
            }
            .font(.ui(size: 12))
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(files) { file in
                        row(file)
                    }
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
            }
            .frame(height: min(CGFloat(files.count) * Self.rowHeight, 192) + 8)
            .box(in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous), bordered: true)
        }
    }

    private func row(_ file: ChangedFile) -> some View {
        let isExcluded = excluded.contains(file.path)
        return HStack(spacing: 8) {
            if editing {
                #if os(macOS)
                Toggle("", isOn: includes(file.path))
                    .toggleStyle(.checkbox)
                    .labelsHidden()
                #else
                ActionButton(
                    icon: isExcluded ? .circle : .circleCheck, help: isExcluded ? "Include" : "Leave out", size: .small,
                    tint: isExcluded ? .themeMutedStrongerForeground : .themePrimary
                ) {
                    includes(file.path).wrappedValue.toggle()
                }
                .padding(.leading, -6)
                #endif
            }
            Text(file.path)
                .lineLimit(1)
                .truncationMode(.head)
                .foregroundStyle(isExcluded ? Color.themeMutedStrongerForeground : Color.themeForeground)
            Spacer(minLength: 8)
            if isExcluded {
                Text("Excluded")
                    .foregroundStyle(Color.themeMutedStrongerForeground)
            } else if file.added + file.removed > 0 {
                LineCounts(added: file.added, removed: file.removed)
            } else {
                Text(file.change == "added" ? "new" : file.change)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
            }
        }
        .font(.ui(size: 12.5))
        .frame(height: Self.rowHeight)
    }

    private func includes(_ path: String) -> Binding<Bool> {
        Binding {
            !excluded.contains(path)
        } set: { included in
            if included { excluded.remove(path) } else { excluded.insert(path) }
        }
    }

    private func commit(onNewBranch: Bool) {
        let paths = excluded.isEmpty ? [] : included
        let written = message.trimmingCharacters(in: .whitespacesAndNewlines)
        close()
        store.runGit("commit", in: project, message: written, paths: paths, newBranch: onNewBranch)
    }
}

/// The lines added and removed, as git counts them.
private struct LineCounts: View {
    let added: Int
    let removed: Int

    var body: some View {
        HStack(spacing: 4) {
            if added > 0 {
                Text("+\(added)").foregroundStyle(Color.themeSuccess)
            }
            if removed > 0 {
                Text("−\(removed)").foregroundStyle(Color.themeDestructive)
            }
        }
        .font(.ui(size: 11, weight: .medium).monospacedDigit())
    }
}
