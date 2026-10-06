import SwiftUI

/// The git button in a thread's top bar. On the Mac its left half does the one thing the
/// repository calls for, at once: commit, push, open a pull request, pull. Its right half opens
/// the menu of all of them, where an item that can't run now says why. While an action runs, the
/// button says which stage it is at. On iOS it is one symbol that opens the menu.
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
            .alert(store.pendingGit?.confirm.title ?? "", isPresented: confirming, presenting: store.pendingGit) { pending in
                Button(pending.confirm.proceed) { store.confirmGit(pending, onNewBranch: false) }
                Button(pending.confirm.branchOff) { store.confirmGit(pending, onNewBranch: true) }
                Button("Cancel", role: .cancel) {}
            } message: { pending in
                Text(pending.confirm.description)
            }
    }

    private func symbol(of quick: GitQuick) -> Symbol {
        guard quick.url == nil else { return project.git?.pullRequest?.state.symbol ?? .gitPullRequest }
        return GitSymbol.symbol(for: quick.action)
    }

    #if os(macOS)
    private var content: some View {
        let stage = store.gitStage(in: project)
        let quick = control.quick
        return HStack(spacing: 0) {
            ActionButton(
                stage?.label ?? quick.title, icon: symbol(of: quick), help: quick.hint ?? project.git?.pullRequest?.title ?? quick.title, variant: .ghost,
                pending: stage != nil, joined: .all, tint: tint(of: quick, at: stage)
            ) {
                store.runQuickGit(in: project)
            }
            Rectangle()
                .fill(Color.themeBorderSecondary)
                .frame(width: 1, height: Self.height)
            menuButton
        }
        .clipShape(RoundedRectangle(cornerRadius: Self.radius, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).stroke(Color.themeBorderSecondary, lineWidth: 1)
        }
        .background(MenuAnchor(anchor: anchor))
        .padding(.horizontal, 6)
    }

    /// A pull request is in the colour of its state, and what has nothing to do is quiet.
    private func tint(of quick: GitQuick, at stage: GitStage?) -> Color? {
        if stage != nil { return .themeText }
        if quick.url != nil, let pullRequest = project.git?.pullRequest { return pullRequest.state.color }
        guard quick.action != nil || quick.url != nil else { return .themeTertiary }
        return .themeText
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
            entry.image = .symbol(GitSymbol.symbol(for: item.action), size: 13)
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
        let showsQuick = (quick.action != nil || quick.url != nil) && !control.menu.contains { $0.label == quick.label }
        return Menu {
            if let stage {
                Section(stage.label) {}
            }
            if showsQuick {
                Section {
                    Button {
                        store.runQuickGit(in: project)
                    } label: {
                        Label(quick.title, symbol: symbol(of: quick), size: 15)
                    }
                    .disabled(running)
                }
            }
            ForEach(control.menu) { item in
                Button {
                    store.chooseGit(item, in: project)
                } label: {
                    Label(item.label, symbol: GitSymbol.symbol(for: item.action), size: 15)
                    if let reason = item.reason { Text(reason) }
                }
                .disabled(item.reason != nil || running)
            }
            if let warning = control.warning {
                Section(warning) {}
            }
        } label: {
            if running {
                Spinner(size: 15)
            } else {
                Image(.gitBranch, size: 15)
            }
        }
        .accessibilityLabel("Git")
    }
    #endif

    private var confirming: Binding<Bool> {
        Binding { store.pendingGit != nil } set: { shown in
            if !shown { store.pendingGit = nil }
        }
    }
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

/// What the last git action did, or what git refused, under the button. What follows is one
/// click away: the push after a commit, the pull request after a push.
struct GitNoticeView: View {
    @Environment(AppStore.self) private var store
    let notice: GitNotice

    private static let radius = Radius.sheet
    private static let padding: CGFloat = 14
    private static let closeSize = ControlSize.small.height
    /// The close button is this far from the top and the right.
    private static let closeMargin: CGFloat = 8
    private static let titleHeight: CGFloat = scaled(16)

    var body: some View {
        let url = notice.url.flatMap { URL(string: $0) }
        HStack(alignment: .top, spacing: 10) {
            Image(notice.failed ? .circleAlert : .circleCheck, size: 15)
                .foregroundStyle(notice.failed ? Color.themeDanger : Color.themeSuccess)
                .frame(height: Self.titleHeight)
            VStack(alignment: .leading, spacing: 4) {
                Text(notice.title)
                    .font(.ui(size: 13, weight: .medium))
                    .foregroundStyle(Color.themeText)
                    .frame(minHeight: Self.titleHeight)
                if let description = notice.description {
                    // The end is where a hook says what it found.
                    Text(description)
                        .font(notice.failed ? .ui(size: 11.5, design: .monospaced) : .ui(size: 12.5))
                        .foregroundStyle(Color.themeSecondary)
                        .lineLimit(notice.failed ? 8 : 2)
                        .truncationMode(notice.failed ? .head : .tail)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if url != nil || notice.nextLabel != nil {
                    HStack(spacing: 8) {
                        if let url {
                            action("View PR", prominent: notice.nextLabel == nil) { store.showPullRequest(url) }
                        }
                        if let next = notice.nextLabel {
                            action(next, prominent: true) { store.runNextGit() }
                        }
                    }
                    .padding(.top, 6)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, Self.padding)
        .padding(.leading, Self.padding)
        .padding(.trailing, Self.closeMargin + Self.closeSize + 4)
        .frame(maxWidth: Platform.scale > 1 ? 440 : 320)
        .background {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous)
                .fill(Color.themePopover)
                .shadow(color: .black.opacity(0.05), radius: 1.5, y: 1)
                .shadow(color: .black.opacity(0.1), radius: 20, y: 8)
        }
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).strokeBorder(Color.themeBorderSecondary, lineWidth: 1)
        }
        .overlay(alignment: .topTrailing) {
            ActionButton(icon: .x, help: "Close", size: .small) { store.dismissGitNotice() }
                .padding(Self.closeMargin)
        }
        .environment(\.surface, .popover)
    }

    private func action(_ title: String, prominent: Bool, run: @escaping () -> Void) -> some View {
        ActionButton(title, variant: prominent ? .primary : .secondary, action: run)
    }
}

/// The sheet behind the menu's Commit: the files to commit, which can be left out one by one,
/// and a message that is written for the user when they leave it empty.
struct CommitSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
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
                Text("Commit changes")
                    .font(.ui(size: 15, weight: .semibold))
                Text("Review and confirm your commit. Leave the message empty to have one written.")
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeSecondary)
            }
            HStack(spacing: 8) {
                Text("Branch")
                    .foregroundStyle(Color.themeSecondary)
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
                ActionButton("Cancel") { dismiss() }
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
                ActionButton("Cancel", variant: .ghost, size: .large, fills: true) { dismiss() }
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
                        .foregroundStyle(Color.themeSecondary)
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
            .layered(in: RoundedRectangle(cornerRadius: Radius.control, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: Radius.control, style: .continuous).strokeBorder(Color.themeBorderSecondary, lineWidth: 1)
            }
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
                    tint: isExcluded ? .themeTertiary : .themePrimary
                ) {
                    includes(file.path).wrappedValue.toggle()
                }
                .padding(.leading, -6)
                #endif
            }
            Text(file.path)
                .lineLimit(1)
                .truncationMode(.head)
                .foregroundStyle(isExcluded ? Color.themeTertiary : Color.themeText)
            Spacer(minLength: 8)
            if isExcluded {
                Text("Excluded")
                    .foregroundStyle(Color.themeTertiary)
            } else if file.added + file.removed > 0 {
                LineCounts(added: file.added, removed: file.removed)
            } else {
                Text(file.change == "added" ? "new" : file.change)
                    .foregroundStyle(Color.themeTertiary)
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
        dismiss()
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
                Text("−\(removed)").foregroundStyle(Color.themeDanger)
            }
        }
        .font(.ui(size: 11, weight: .medium).monospacedDigit())
    }
}
