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
    private static let height = ToolbarButton.width - 2 * ToolbarButton.margin
    private static let radius: CGFloat = 7
    /// What leaves the room under the button that the composer's menus leave over theirs.
    private static let menuGap: CGFloat = 16
    @State private var anchor = MenuAnchorView()
    #endif

    var body: some View {
        content
            .alert(store.pendingGit?.confirm.title ?? "", isPresented: confirming, presenting: store.pendingGit) { pending in
                Button(pending.confirm.proceed) { store.confirmGit(pending, onNewBranch: false) }
                Button(pending.confirm.branchOff) { store.confirmGit(pending, onNewBranch: true) }
                Button("Abort", role: .cancel) {}
            } message: { pending in
                Text(pending.confirm.description)
            }
    }

    #if os(macOS)
    private var content: some View {
        let stage = store.gitStages[project.id]
        let quick = control.quick
        return HStack(spacing: 0) {
            Button {
                store.runQuickGit(in: project)
            } label: {
                HStack(spacing: 6) {
                    if stage != nil {
                        ProgressView()
                            .controlSize(.small)
                            .scaleEffect(0.7)
                            .frame(width: 14, height: 14)
                    } else {
                        Image(systemName: GitSymbol.name(for: quick.action))
                            .font(.ui(size: 12, weight: .medium))
                    }
                    Text(stage?.label ?? quick.title)
                        .font(.ui(size: 12, weight: .medium))
                        .lineLimit(1)
                }
                .foregroundStyle(color(of: quick, at: stage))
                .padding(.horizontal, 9)
                .frame(height: Self.height)
                .contentShape(Rectangle())
            }
            .buttonStyle(.highlight(radius: 0))
            .disabled(stage != nil)
            .help(quick.hint ?? project.git?.pullRequest?.title ?? quick.title)
            Rectangle()
                .fill(Color.themeStrongBorder)
                .frame(width: 1, height: Self.height)
            menuButton
        }
        .clipShape(RoundedRectangle(cornerRadius: Self.radius, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
        }
        .background(MenuAnchor(anchor: anchor))
        .padding(.horizontal, 6)
    }

    private func color(of quick: GitQuick, at stage: GitStage?) -> Color {
        if stage != nil { return .themeText }
        if quick.state != nil { return .themeMerged }
        guard quick.action != nil || quick.url != nil else { return .themeTertiary }
        return .themeText
    }

    private var chevron: some View {
        Image(systemName: "chevron.down")
            .font(.ui(size: 9, weight: .bold))
            .foregroundStyle(Color.themeSecondary)
            .frame(width: 24, height: Self.height)
            .contentShape(Rectangle())
    }

    private var menuButton: some View {
        Button(action: showMenu) { chevron }
            .buttonStyle(.highlight(radius: 0))
            .help("Commit, push or open a pull request")
    }

    /// Opens the menu under the button, its right edge on the button's. An item that can't run
    /// now is greyed, and says why when the pointer rests on it.
    private func showMenu() {
        guard let view = anchor.view else { return }
        let menu = NSMenu()
        menu.autoenablesItems = false
        let running = store.gitStages[project.id] != nil
        for item in control.menu {
            let entry = ActionMenuItem(title: item.label) { store.chooseGit(item, in: project) }
            entry.image = NSImage(systemSymbolName: GitSymbol.name(for: item.action), accessibilityDescription: nil)
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
        let running = store.gitStages[project.id] != nil
        let quick = control.quick
        let showsQuick = (quick.action != nil || quick.url != nil) && !control.menu.contains { $0.label == quick.label }
        return Menu {
            if showsQuick {
                Section {
                    Button {
                        store.runQuickGit(in: project)
                    } label: {
                        Label(quick.title, systemImage: GitSymbol.name(for: quick.action))
                    }
                    .disabled(running)
                }
            }
            ForEach(control.menu) { item in
                Button {
                    store.chooseGit(item, in: project)
                } label: {
                    Label(item.label, systemImage: GitSymbol.name(for: item.action))
                    if let reason = item.reason { Text(reason) }
                }
                .disabled(item.reason != nil || running)
            }
            if let warning = control.warning {
                Section(warning) {}
            }
        } label: {
            if running {
                ProgressView()
            } else {
                Image(systemName: "arrow.triangle.branch")
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

    private static let radius: CGFloat = 10
    private static let padding: CGFloat = 12
    private static let closeSize: CGFloat = 24
    /// The close button is this far from the top and the right, and its corners follow the notice's.
    private static let closeMargin: CGFloat = 5

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(notice.title)
                .font(.ui(size: 12.5, weight: .semibold))
                .foregroundStyle(Color.themeText)
                .padding(.trailing, Self.closeMargin + Self.closeSize + 4 - Self.padding)
            if let description = notice.description {
                // The end is where a hook says what it found.
                Text(description)
                    .font(.ui(size: 12, design: notice.failed ? .monospaced : .default))
                    .foregroundStyle(notice.failed ? Color.themeDanger : Color.themeSecondary)
                    .lineLimit(notice.failed ? 8 : 2)
                    .truncationMode(notice.failed ? .head : .tail)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let url = notice.url.flatMap({ URL(string: $0) }) {
                LinkButton("View PR") { Platform.open(url) }
            }
            if let next = notice.nextLabel {
                LinkButton(next) { store.runNextGit() }
            }
        }
        .padding(Self.padding)
        .frame(maxWidth: 300, alignment: .leading)
        .background(Color.themeRaised, in: RoundedRectangle(cornerRadius: Self.radius, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
        }
        .overlay(alignment: .topTrailing) {
            IconOnlyButton(symbol: "xmark", help: "Close", size: Self.closeSize, symbolSize: 11, radius: Self.radius - Self.closeMargin) {
                store.dismissGitNotice()
            }
            .foregroundStyle(Color.themeSecondary)
            .padding(Self.closeMargin)
        }
        .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
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
                CommitMessageBox(text: $message)
            }
            #if os(macOS)
            HStack(spacing: 8) {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Commit on New Branch") { commit(onNewBranch: true) }
                    .disabled(included.isEmpty)
                Button("Commit") { commit(onNewBranch: false) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(included.isEmpty)
            }
            #else
            VStack(spacing: 8) {
                Button {
                    commit(onNewBranch: false)
                } label: {
                    Text("Commit").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(included.isEmpty)
                Button {
                    commit(onNewBranch: true)
                } label: {
                    Text("Commit on New Branch").frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .disabled(included.isEmpty)
                Button("Cancel", role: .cancel) { dismiss() }
                    .padding(.top, 4)
            }
            .controlSize(.large)
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
                if editing {
                    LinkButton(excluded.isEmpty ? "Select None" : "Select All") {
                        excluded = excluded.isEmpty ? Set(files.map(\.path)) : []
                    }
                    .padding(.trailing, 2 * LinkButton.padding - 8)
                }
                LinkButton(editing ? "Done" : "Edit") { editing.toggle() }
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
            .background(Color.themeField, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
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
                Button {
                    includes(file.path).wrappedValue.toggle()
                } label: {
                    Image(systemName: isExcluded ? "circle" : "checkmark.circle.fill")
                        .font(.system(size: 20))
                        .foregroundStyle(isExcluded ? Color.themeTertiary : Color.themePrimary)
                }
                .buttonStyle(.plain)
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

private struct CommitMessageBox: View {
    @Binding var text: String

    var body: some View {
        TextEditor(text: $text)
            .font(.ui(size: 12.5))
            .scrollContentBackground(.hidden)
            .padding(.horizontal, 4)
            .padding(.vertical, 6)
            .frame(height: 84)
            .background(Color.themeField, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
            }
            .overlay(alignment: .topLeading) {
                if text.isEmpty {
                    Text("Leave empty to have one written")
                        .font(.ui(size: 12.5))
                        .foregroundStyle(Color.themeTertiary)
                        .padding(.horizontal, 9)
                        .padding(.vertical, 6)
                        .allowsHitTesting(false)
                }
            }
    }
}
