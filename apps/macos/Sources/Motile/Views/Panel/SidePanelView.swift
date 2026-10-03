import SwiftUI

/// The panel beside the thread: its tabs in the window's top bar, and under them what the open
/// tab shows of the folder the thread works in.
struct SidePanelView: View {
    @Environment(AppStore.self) private var store
    /// The height of the window's top bar, which the tabs are drawn in.
    let topInset: CGFloat
    /// How far the tabs start from the panel's left edge: past the window's buttons when the
    /// panel reaches them.
    var tabInset: CGFloat = 0

    var body: some View {
        let tabs = store.sidePanel.tabs
        VStack(spacing: 0) {
            PanelLine()
            content(tabs.active)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .overlay(alignment: .top) {
            PanelTabStrip(tabs: tabs)
                .padding(.leading, tabInset)
                .frame(height: topInset)
                .offset(y: -topInset)
        }
    }

    @ViewBuilder
    private func content(_ active: PanelTab?) -> some View {
        if let reason = store.panelUnavailable {
            PanelMessage(text: reason)
        } else if let target = store.panelTarget {
            switch active {
            case .diff: DiffSurface(target: target)
            case .files: FilesSurface(target: target)
            case .file(let path): FileSurface(target: target, path: path).id(path)
            case nil: PanelLauncher(target: target)
            }
        }
    }
}

struct PanelLine: View {
    var body: some View {
        Rectangle()
            .fill(Color.themeBorder)
            .frame(height: 1)
    }
}

/// What a tab says in the middle of the panel when it has nothing to show.
struct PanelMessage: View {
    let text: String
    var failed = false

    var body: some View {
        Text(text)
            .font(.system(size: 12.5))
            .foregroundStyle(failed ? Color.themeDanger : Color.themeSecondary)
            .multilineTextAlignment(.center)
            .textSelection(.enabled)
            .padding(24)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct PanelLoading: View {
    var body: some View {
        ProgressView()
            .controlSize(.small)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// The strip under the tabs with what the open tab is about and its buttons.
struct PanelBar<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 4) { content }
                .padding(.leading, 12)
                .padding(.trailing, 6)
                .frame(height: 36)
            PanelLine()
        }
    }
}

/// Said over what a tab shows when that is only a part of it.
struct PanelNote: View {
    let text: String

    var body: some View {
        VStack(spacing: 0) {
            Text(text)
                .font(.system(size: 12))
                .foregroundStyle(Color.themeWarning)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(Color(nsColor: Theme.warningBackground))
            PanelLine()
        }
    }
}

private struct PanelTabStrip: View {
    @Environment(AppStore.self) private var store
    let tabs: PanelTabs

    var body: some View {
        ScrollViewReader { strip in
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 2) {
                    ForEach(tabs.tabs) { tab in
                        PanelTabChip(tab: tab, active: tab == tabs.active)
                    }
                    if !tabs.tabs.isEmpty {
                        add
                    }
                }
                .padding(.horizontal, 8)
                .frame(maxHeight: .infinity)
            }
            .onChange(of: tabs.active, initial: true) {
                guard let active = tabs.active else { return }
                strip.scrollTo(active.id)
            }
        }
        // The buttons that maximize and hide the panel are at the window's edge.
        .padding(.trailing, 2 * ToolbarButton.width + 14)
    }

    private var add: some View {
        Menu {
            Button("Files") { store.sidePanel.open(.files) }
            Button("Diff") { store.sidePanel.showDiff() }
                .disabled(store.panelTarget?.repository != true)
        } label: {
            Image(systemName: "plus")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(Color.themeSecondary)
                .frame(width: 28, height: 28)
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight()
        .help("Open a tab")
    }
}

private struct PanelTabChip: View {
    @Environment(AppStore.self) private var store
    let tab: PanelTab
    let active: Bool
    @State private var hovering = false

    var body: some View {
        let panel = store.sidePanel
        HStack(spacing: 6) {
            Image(systemName: tab.symbol)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(active ? Color.themeText : Color.themeSecondary)
                .frame(width: 14)
            Text(tab.title)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(active ? Color.themeText : Color.themeSecondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Button {
                panel.close(tab)
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(Color.themeSecondary)
                    .frame(width: 16, height: 16)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .hoverHighlight(radius: 4)
            .opacity(hovering || active ? 1 : 0)
            .help("Close (⌘W)")
        }
        .padding(.leading, 9)
        .padding(.trailing, 6)
        .frame(height: 28)
        .frame(maxWidth: 180)
        .contentShape(Rectangle())
        .onTapGesture { panel.activate(tab) }
        .hoverHighlight(selected: active)
        .onHover { hovering = $0 }
        .help(path ?? tab.title)
        .contextMenu {
            Button("Close") { panel.close(tab) }
            Button("Close Others") { panel.closeOthers(tab) }
            Button("Close All") { panel.closeAll() }
            if let path {
                Divider()
                Button("Copy Path") { Pasteboard.copy(path) }
            }
        }
    }

    private var path: String? {
        guard case .file(let path) = tab else { return nil }
        return path
    }
}

enum Pasteboard {
    static func copy(_ text: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}

/// What the panel shows before a tab is open: the tabs there are to open.
private struct PanelLauncher: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget

    var body: some View {
        VStack(spacing: 12) {
            Text("Open a tab")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Color.themeText)
            VStack(spacing: 2) {
                row("folder", "Files", keys: "⇧⌘E", reason: nil) { store.sidePanel.open(.files) }
                row("plusminus", "Diff", keys: "⌘D", reason: target.repository ? nil : "Available in git repositories.") {
                    store.sidePanel.showDiff()
                }
            }
            .frame(width: 250)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func row(_ symbol: String, _ title: String, keys: String, reason: String?, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: symbol)
                    .font(.system(size: 13, weight: .medium))
                    .frame(width: 18)
                Text(title)
                    .font(.system(size: 13))
                Spacer()
                Text(keys)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(Color.themeSecondary)
                    .padding(.horizontal, 6)
                    .frame(height: 20)
                    .background(Color.themeHover, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
            }
            .foregroundStyle(Color.themeText)
            .padding(.horizontal, 10)
            .frame(height: 34)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .hoverHighlight()
        .disabled(reason != nil)
        .opacity(reason == nil ? 1 : 0.45)
        .help(reason ?? "")
    }
}

/// What makes a tab ask its server again: another folder, a turn that ended there, or the
/// button that asks.
struct PanelTrigger: Equatable {
    let target: PanelTarget
    var scope: DiffScope?
    var path: String?
    let version: Int
    let asked: Int
}

/// The changes in the thread's folder: what isn't committed, what the branch adds, or what one
/// turn did.
struct DiffSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    @State private var asked = 0

    var body: some View {
        let panel = store.sidePanel
        let scope = panel.scope(for: target)
        VStack(spacing: 0) {
            PanelBar {
                scopeMenu(scope)
                if let document = panel.diff.value, !document.files.isEmpty {
                    Text(AttributedString(LineCountText.text(added: document.added, removed: document.removed)))
                        .padding(.leading, 4)
                }
                Spacer(minLength: 4)
                if let document = panel.diff.value, document.files.count > 1 {
                    let allClosed = panel.collapsed.count >= document.files.count
                    IconOnlyButton(
                        symbol: allClosed ? "rectangle.expand.vertical" : "rectangle.compress.vertical",
                        help: allClosed ? "Open every file" : "Close every file"
                    ) {
                        panel.setAllCollapsed(!allClosed)
                    }
                }
                IconOnlyButton(symbol: "arrow.clockwise", help: "Read the changes again") { asked += 1 }
            }
            if !target.repository {
                PanelMessage(text: "This folder isn't a git repository.")
            } else {
                changes(panel.diff, scope: scope)
            }
        }
        .task(id: PanelTrigger(target: target, scope: scope, version: store.workspaceVersion, asked: asked)) {
            guard target.repository else { return }
            panel.loadDiff(of: target, scope: scope)
        }
    }

    @ViewBuilder
    private func changes(_ diff: Loaded<CodeDocument>, scope: DiffScope) -> some View {
        let panel = store.sidePanel
        switch diff {
        case .loading:
            PanelLoading()
        case .failed(let message):
            PanelMessage(text: message, failed: true)
        case .ready(let document) where document.files.isEmpty:
            PanelMessage(text: scope == .uncommitted ? "Everything is committed." : "Nothing has changed.")
        case .ready(let document):
            if document.truncated {
                PanelNote(text: "These changes are too long to show in full. This is their start.")
            }
            CodeViewRepresentable(
                document: document, collapsed: panel.collapsed, reveal: panel.reveal,
                onToggle: { panel.toggleCollapsed($0) }, onOpenFile: { panel.open(.file($0)) })
        }
    }

    private func scopeMenu(_ scope: DiffScope) -> some View {
        let panel = store.sidePanel
        return Menu {
            Toggle("Uncommitted changes", isOn: chosen(.uncommitted, scope))
            Toggle("Branch changes", isOn: chosen(.branch, scope))
            if !panel.turns.isEmpty {
                Divider()
                ForEach(panel.turns.reversed()) { turn in
                    Toggle(label(of: turn), isOn: chosen(.turn(turn.id), scope))
                }
            }
        } label: {
            HStack(spacing: 5) {
                Text(title(of: scope))
                    .font(.system(size: 12.5, weight: .medium))
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(Color.themeTertiary)
            }
            .foregroundStyle(Color.themeText)
            .padding(.horizontal, 8)
            .frame(height: 26)
            .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight()
        .padding(.leading, -8)
    }

    private func chosen(_ option: DiffScope, _ scope: DiffScope) -> Binding<Bool> {
        Binding { option == scope } set: { _ in store.sidePanel.choose(option) }
    }

    private func title(of scope: DiffScope) -> String {
        switch scope {
        case .uncommitted: return "Uncommitted"
        case .branch: return "Branch"
        case .turn(let id):
            let turns = store.sidePanel.turns
            guard let turn = turns.first(where: { $0.id == id }), turn.id != turns.last?.id else { return "Latest turn" }
            return "Turn at \(time(turn.at))"
        }
    }

    private func label(of turn: TurnChange) -> String {
        let name = turn.id == store.sidePanel.turns.last?.id ? "Latest turn" : "Turn at \(time(turn.at))"
        return "\(name) · \(turn.files == 1 ? "1 file" : "\(turn.files) files")"
    }

    private func time(_ at: Double) -> String {
        let date = Date(timeIntervalSince1970: at)
        let today = Calendar.current.isDateInToday(date)
        return date.formatted(date: today ? .omitted : .abbreviated, time: .shortened)
    }
}

/// The folder the thread works in: its files under their folders, to open one in a tab.
struct FilesSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    @State private var asked = 0

    var body: some View {
        let panel = store.sidePanel
        VStack(spacing: 0) {
            PanelBar {
                Image(systemName: "folder")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(Color.themeSecondary)
                Text(target.name)
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(Color.themeText)
                    .lineLimit(1)
                    .padding(.leading, 3)
                Spacer(minLength: 4)
                IconOnlyButton(symbol: "arrow.clockwise", help: "Read the folder again") { asked += 1 }
            }
            if let error = panel.filesError {
                PanelMessage(text: error, failed: true)
            } else if let top = panel.listings[""] {
                if top.isEmpty {
                    PanelMessage(text: "This folder is empty.")
                } else {
                    ScrollView {
                        LazyVStack(spacing: 0) {
                            ForEach(panel.nodes) { node in
                                FileRow(node: node)
                            }
                        }
                        .padding(.vertical, 6)
                    }
                }
            } else {
                PanelLoading()
            }
        }
        .task(id: PanelTrigger(target: target, version: store.workspaceVersion, asked: asked)) {
            panel.loadFiles(of: target)
        }
    }
}

private struct FileRow: View {
    @Environment(AppStore.self) private var store
    let node: FileNode

    var body: some View {
        let panel = store.sidePanel
        HStack(spacing: 6) {
            Image(systemName: node.open ? "chevron.down" : "chevron.right")
                .font(.system(size: 8, weight: .bold))
                .foregroundStyle(Color.themeTertiary)
                .frame(width: 10)
                .opacity(node.folder ? 1 : 0)
            Image(systemName: node.folder ? "folder" : FileSymbol.name(for: node.path))
                .font(.system(size: 11))
                .foregroundStyle(Color.themeSecondary)
                .frame(width: 16)
            Text(node.name)
                .font(.system(size: 12.5))
                .foregroundStyle(Color.themeText)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 0)
        }
        .opacity(node.ignored ? 0.5 : 1)
        .padding(.leading, 12 + CGFloat(node.depth) * 14)
        .padding(.trailing, 10)
        .frame(height: 26)
        .contentShape(Rectangle())
        .onTapGesture {
            if node.folder { panel.toggleFolder(node.path) } else { panel.open(.file(node.path)) }
        }
        .hoverHighlight(radius: 6, inset: EdgeInsets(top: 0, leading: 6, bottom: 0, trailing: 6))
        .contextMenu {
            Button("Copy Path") { Pasteboard.copy(node.path) }
        }
    }
}

/// One file of the folder the thread works in.
struct FileSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    let path: String
    @State private var asked = 0
    @State private var copied = false

    var body: some View {
        let panel = store.sidePanel
        VStack(spacing: 0) {
            PanelBar {
                (Text(folder).foregroundStyle(Color.themeSecondary) + Text((path as NSString).lastPathComponent).foregroundStyle(Color.themeText))
                    .font(.system(size: 12.5, weight: .medium))
                    .lineLimit(1)
                    .truncationMode(.head)
                Spacer(minLength: 4)
                IconOnlyButton(symbol: copied ? "checkmark" : "doc.on.doc", help: "Copy the path") {
                    Pasteboard.copy(path)
                    copied = true
                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { copied = false }
                }
                IconOnlyButton(symbol: "arrow.clockwise", help: "Read the file again") { asked += 1 }
            }
            switch panel.contents[path] {
            case nil, .loading:
                PanelLoading()
            case .failed(let message):
                PanelMessage(text: message, failed: true)
            case .ready(.text(let document, let truncated)):
                if truncated {
                    PanelNote(text: "This file is too long to show in full. This is its start.")
                }
                if document.files.first?.lines.isEmpty == true {
                    PanelMessage(text: "This file is empty.")
                } else {
                    CodeViewRepresentable(document: document)
                }
            case .ready(.image(let image)):
                ZoomableImage(image: image, size: image.size, margin: CGSize(width: 16, height: 16), keys: store.viewing == nil)
            case .ready(.binary(let size)):
                PanelMessage(text: "This file isn't text, so it isn't shown. It is \(ByteCountFormatter.string(fromByteCount: Int64(size), countStyle: .file)).")
            }
        }
        .task(id: PanelTrigger(target: target, path: path, version: store.workspaceVersion, asked: asked)) {
            panel.loadFile(path, of: target)
        }
    }

    private var folder: String {
        let folder = (path as NSString).deletingLastPathComponent
        return folder.isEmpty ? "" : folder + "/"
    }
}
