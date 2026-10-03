import SwiftUI

/// The git button in a thread's top bar. Its left half does the one thing the repository calls
/// for, at once: commit, push, open a pull request, pull. Its right half opens the menu of all
/// of them, where an item that can't run now says why. While an action runs, the button says
/// which stage it is at.
struct GitButton: View {
    @Environment(AppStore.self) private var store
    let project: Project
    let control: GitControl

    private static let height = ToolbarButton.width - 2 * ToolbarButton.margin
    /// What leaves the room under the button that the composer's menus leave over theirs.
    private static let menuGap: CGFloat = 16
    @State private var anchor = MenuAnchorView()

    var body: some View {
        @Bindable var store = store
        let stage = store.gitStages[project.id]
        let quick = control.quick
        let runs = quick.action != nil || quick.url != nil
        HStack(spacing: 0) {
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
                            .font(.system(size: 12, weight: .medium))
                    }
                    Text(stage?.label ?? quick.label)
                        .font(.system(size: 12, weight: .medium))
                }
                .foregroundStyle(runs || stage != nil ? Color.themeText : Color.themeTertiary)
                .padding(.horizontal, 9)
                .frame(height: Self.height)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .hoverHighlight(radius: 0)
            .disabled(stage != nil)
            .help(quick.hint ?? project.git?.pullRequest?.title ?? quick.label)
            Rectangle()
                .fill(Color.themeStrongBorder)
                .frame(width: 1, height: Self.height)
            Button(action: showMenu) {
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Color.themeSecondary)
                    .frame(width: 24, height: Self.height)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .hoverHighlight(radius: 0)
            .help("Commit, push or open a pull request")
        }
        .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
        }
        .background(MenuAnchor(anchor: anchor))
        .padding(.horizontal, 6)
        .alert(store.pendingGit?.confirm.title ?? "", isPresented: confirming, presenting: store.pendingGit) { pending in
            Button(pending.confirm.proceed) { store.confirmGit(pending, onNewBranch: false) }
            Button(pending.confirm.branchOff) { store.confirmGit(pending, onNewBranch: true) }
            Button("Abort", role: .cancel) {}
        } message: { pending in
            Text(pending.confirm.description)
        }
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

    private var confirming: Binding<Bool> {
        Binding { store.pendingGit != nil } set: { shown in
            if !shown { store.pendingGit = nil }
        }
    }
}

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
                .font(.system(size: 12.5, weight: .semibold))
                .foregroundStyle(Color.themeText)
                .padding(.trailing, Self.closeMargin + Self.closeSize + 4 - Self.padding)
            if let description = notice.description {
                // The end is where a hook says what it found.
                Text(description)
                    .font(.system(size: 12, design: notice.failed ? .monospaced : .default))
                    .foregroundStyle(notice.failed ? Color.themeDanger : Color.themeSecondary)
                    .lineLimit(notice.failed ? 8 : 2)
                    .truncationMode(notice.failed ? .head : .tail)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let url = notice.url.flatMap({ URL(string: $0) }) {
                Button("View PR") { NSWorkspace.shared.open(url) }
                    .buttonStyle(.link)
                    .font(.system(size: 12))
            }
            if let next = notice.nextLabel {
                Button(next) { store.runNextGit() }
                    .buttonStyle(.link)
                    .font(.system(size: 12))
            }
        }
        .padding(Self.padding)
        .frame(width: 300, alignment: .leading)
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

    private static let rowHeight: CGFloat = 24

    private var files: [ChangedFile] { store.gitFiles }
    private var included: [String] { files.map(\.path).filter { !excluded.contains($0) } }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Commit changes")
                    .font(.system(size: 15, weight: .semibold))
                Text("Review and confirm your commit. Leave the message empty to have one written.")
                    .font(.system(size: 12))
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
            .font(.system(size: 12.5))
            fileList
            VStack(alignment: .leading, spacing: 6) {
                Text("Commit message (optional)")
                    .font(.system(size: 12.5, weight: .medium))
                CommitMessageBox(text: $message)
            }
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
        }
        .padding(20)
        .frame(width: 460)
    }

    private var fileList: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text("Files")
                    .font(.system(size: 12.5, weight: .medium))
                if !excluded.isEmpty, !editing {
                    Text("\(included.count) of \(files.count)")
                        .font(.system(size: 12))
                        .foregroundStyle(Color.themeSecondary)
                }
                Spacer()
                if editing {
                    Button(excluded.isEmpty ? "Select None" : "Select All") {
                        excluded = excluded.isEmpty ? Set(files.map(\.path)) : []
                    }
                    .buttonStyle(.link)
                }
                Button(editing ? "Done" : "Edit") { editing.toggle() }
                    .buttonStyle(.link)
            }
            .font(.system(size: 12))
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
            .background(Color.themeComposer, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
            }
        }
    }

    private func row(_ file: ChangedFile) -> some View {
        let isExcluded = excluded.contains(file.path)
        return HStack(spacing: 8) {
            if editing {
                Toggle("", isOn: includes(file.path))
                    .toggleStyle(.checkbox)
                    .labelsHidden()
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
        .font(.system(size: 12.5))
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
        .font(.system(size: 11, weight: .medium).monospacedDigit())
    }
}

private struct CommitMessageBox: View {
    @Binding var text: String

    var body: some View {
        TextEditor(text: $text)
            .font(.system(size: 12.5))
            .scrollContentBackground(.hidden)
            .padding(.horizontal, 4)
            .padding(.vertical, 6)
            .frame(height: 84)
            .background(Color.themeComposer, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
            }
            .overlay(alignment: .topLeading) {
                if text.isEmpty {
                    Text("Leave empty to have one written")
                        .font(.system(size: 12.5))
                        .foregroundStyle(Color.themeTertiary)
                        .padding(.horizontal, 9)
                        .padding(.vertical, 6)
                        .allowsHitTesting(false)
                }
            }
    }
}
