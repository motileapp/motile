import SwiftUI

/// A strip against the composer's top or bottom, on the composer's surface: rounded on
/// its outer corners and open where it meets the composer. It stands in from the composer's
/// sides by the composer's corner radius, so it meets the composer's straight edge.
struct ComposerStrip: ViewModifier {
    enum Edge {
        case top, bottom
    }

    static let height: CGFloat = Platform.scale > 1 ? 38 : 32
    static let radius: CGFloat = 14
    /// The room around a control in a strip, which is the control's to click.
    static let margin = EdgeInsets(top: 4, leading: 4, bottom: 4, trailing: 8)

    let edge: Edge

    func body(content: Content) -> some View {
        content
            .frame(height: Self.height)
            .composerSurface(in: StripShape(edge: edge))
            .padding(.horizontal, ComposerView.radius)
            .padding(edge == .top ? .bottom : .top, -Self.overlap)
    }

    /// How far the strip's open edge goes under the composer, which covers its outline there.
    static let overlap: CGFloat = 1
}

/// The strip's outline, rounded on the two outer corners.
private struct StripShape: Shape {
    let edge: ComposerStrip.Edge

    func path(in rect: CGRect) -> Path {
        let radius = ComposerStrip.radius
        var path = Path()
        switch edge {
        case .top:
            path.move(to: CGPoint(x: rect.minX, y: rect.maxY))
            path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
            path.addArc(center: CGPoint(x: rect.minX + radius, y: rect.minY + radius), radius: radius, startAngle: .degrees(180), endAngle: .degrees(270), clockwise: false)
            path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
            path.addArc(center: CGPoint(x: rect.maxX - radius, y: rect.minY + radius), radius: radius, startAngle: .degrees(270), endAngle: .degrees(360), clockwise: false)
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        case .bottom:
            path.move(to: CGPoint(x: rect.minX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.minX, y: rect.maxY - radius))
            path.addArc(center: CGPoint(x: rect.minX + radius, y: rect.maxY - radius), radius: radius, startAngle: .degrees(180), endAngle: .degrees(90), clockwise: true)
            path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.maxY))
            path.addArc(center: CGPoint(x: rect.maxX - radius, y: rect.maxY - radius), radius: radius, startAngle: .degrees(90), endAngle: .degrees(0), clockwise: true)
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
        }
        path.closeSubpath()
        return path
    }
}

#if os(macOS)
/// Where the composer's thread works: the server, the folder or a worktree of its own, and the
/// branch checked out there, which opens the picker to switch. A thread that starts in a new
/// worktree picks the branch it starts from there instead. On iOS the thread's settings say it.
struct ContextStrip: View {
    @Environment(AppStore.self) private var store
    let project: Project
    let server: Server?

    var body: some View {
        parts
            .modifier(ComposerStrip(edge: .bottom))
    }

    private var parts: some View {
        HStack(spacing: 0) {
            if let server {
                part(server.name) {
                    Image(.server, size: 11)
                }
                .padding(.leading, 14)
                .help("On \(server.name)")
                divider
                    .padding(.horizontal, 10)
            }
            part(project.name) {
                ProjectIcon(project: project, size: 13)
            }
            .padding(.leading, server == nil ? 14 : 0)
            .help(project.path)
            Spacer(minLength: 8)
            workspace
            if hasWorkspace, branch != nil {
                divider
            }
            branchPart
        }
    }

    private var branch: String? {
        store.draftUsesWorktree ? store.draftBase : project.branch
    }

    private var hasWorkspace: Bool {
        project.worktree != nil
            || (store.selectedThread != nil && project.branch != nil)
            || (store.selectedThread == nil && store.canUseWorktrees(of: project))
    }

    @ViewBuilder private var branchPart: some View {
        @Bindable var store = store
        let startsInWorktree = store.draftUsesWorktree
        if let branch {
            if project.worktree != nil {
                branchLabel(branch, opens: false)
                    .foregroundStyle(Color.themeSecondary)
                    .help("The branch of this thread's worktree")
            } else if store.canSwitchBranches(of: project) {
                Button {
                    store.showBranches(of: project)
                } label: {
                    branchLabel(startsInWorktree ? "From \(branch)" : branch, opens: true)
                }
                .buttonStyle(.highlight(radius: 7, inset: ComposerStrip.margin, faded: true))
                .help(startsInWorktree ? "The branch the worktree's branch starts from" : "Switch the branch of \(project.name)")
                .popover(isPresented: $store.showsBranches, arrowEdge: .bottom) {
                    BranchPicker(project: project, base: startsInWorktree ? branch : nil)
                }
            } else {
                branchLabel(branch, opens: false)
                    .foregroundStyle(Color.themeSecondary)
                    .help(server?.known == true ? "Update \(server?.name ?? "your server") to switch branches from here" : "The branch checked out there")
            }
        }
    }

    /// Where a new thread starts, to choose, and where a thread that has started works.
    @ViewBuilder private var workspace: some View {
        if let worktree = project.worktree {
            working(in: "Worktree", symbol: .folderGit2)
                .help(worktree.path)
        } else if store.selectedThread != nil, project.branch != nil {
            working(in: "Local checkout", symbol: .folder)
                .help("The thread works in the project's folder")
        } else if store.selectedThread == nil, store.canUseWorktrees(of: project) {
            let inWorktree = store.draftUsesWorktree
            let margin = EdgeInsets(top: 4, leading: 4, bottom: 4, trailing: 4)
            Menu {
                Section("Workspace") {
                    Toggle(isOn: Binding(get: { !inWorktree }, set: { _ in store.setDraftWorktree(false) })) {
                        Label("Current checkout", symbol: .folder)
                    }
                    Toggle(isOn: Binding(get: { inWorktree }, set: { _ in store.setDraftWorktree(true) })) {
                        Label("New worktree", symbol: .folderGit2)
                    }
                }
            } label: {
                HStack(spacing: 5) {
                    Image(inWorktree ? .folderGit2 : .folder, size: 11)
                    Text(inWorktree ? "New worktree" : "Current checkout")
                        .font(.ui(size: 12))
                        .lineLimit(1)
                    Image(.chevronDown, size: 9)
                        .foregroundStyle(Color.themeTertiary)
                }
                .padding(.horizontal, 9)
                .frame(height: 24)
                .padding(margin)
                .contentShape(Rectangle())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .hoverHighlight(radius: 7, inset: margin, faded: true)
            .help(inWorktree ? "The thread works in a folder and on a branch of its own" : "The thread works in the project's folder")
        }
    }

    private func working(in title: String, symbol: Symbol) -> some View {
        part(title) {
            Image(symbol, size: 11)
        }
        .padding(.horizontal, 13)
    }

    private var divider: some View {
        Rectangle()
            .fill(Color.themeBorder)
            .frame(width: 1, height: 14)
    }

    private func part(_ title: String, @ViewBuilder icon: () -> some View) -> some View {
        HStack(spacing: 5) {
            icon()
            Text(title)
                .font(.ui(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .foregroundStyle(Color.themeSecondary)
    }

    private func branchLabel(_ branch: String, opens: Bool) -> some View {
        HStack(spacing: 5) {
            Image(.gitBranch, size: 11)
            Text(branch)
                .font(.ui(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
            if opens {
                Image(.chevronDown, size: 9)
                    .foregroundStyle(Color.themeTertiary)
            }
        }
        .padding(.horizontal, 9)
        .frame(height: 24)
        .padding(ComposerStrip.margin)
        .contentShape(Rectangle())
    }
}
#endif

/// The branches of the project's repository, to switch to one or make a new one. What is typed
/// narrows the list, and becomes the name of a branch to make when it matches none. With `base`
/// it picks the branch a new worktree starts from instead, and switches nothing.
struct BranchPicker: View {
    @Environment(AppStore.self) private var store
    let project: Project
    var base: String?
    @State private var query = ""
    @State private var problem: String?
    @State private var highlighted = 0
    @State private var switching = false
    @FocusState private var searching: Bool

    private static let rowHeight: CGFloat = Platform.scale > 1 ? 44 : 30
    private static let listPadding: CGFloat = 8
    private static let maxListHeight: CGFloat = Platform.scale > 1 ? 420 : 300

    private enum Choice: Identifiable {
        case branch(Branch)
        case create(String)

        var id: String {
            switch self {
            case .branch(let branch): "branch:\(branch.name)"
            case .create(let name): "create:\(name)"
            }
        }
    }

    private var working: Bool { base == nil && store.isWorking(in: project) }

    private var branches: [Branch]? { try? store.listedBranches.get() }

    /// The height of all the branches, also while fewer match: a popover that shrinks leaves
    /// the button it opened from.
    private var listHeight: CGFloat {
        min(Self.maxListHeight, CGFloat(branches?.count ?? 0) * Self.rowHeight + 2 * Self.listPadding)
    }

    private var listProblem: String? {
        guard case .failure(let error) = store.listedBranches else { return nil }
        return error.message
    }

    private var choices: [Choice] {
        guard let branches else { return [] }
        let needle = query.trimmingCharacters(in: .whitespaces)
        let matching = branches.filter { needle.isEmpty || $0.name.localizedCaseInsensitiveContains(needle) }
        var choices = matching.map(Choice.branch)
        if base == nil, !needle.isEmpty, !branches.contains(where: { $0.name == needle }) {
            choices.append(.create(needle))
        }
        return choices
    }

    var body: some View {
        let choices = self.choices
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Image(.search, size: 12)
                    .foregroundStyle(Color.themeTertiary)
                TextField(base == nil ? "Switch or create a branch…" : "Start from a branch…", text: $query)
                    .textFieldStyle(.plain)
                    .font(.ui(size: 13))
                    .focused($searching)
                    .onSubmit { choose(choices, at: highlighted) }
                    .onKeyPress(.downArrow) {
                        highlighted = min(highlighted + 1, max(0, choices.count - 1))
                        return .handled
                    }
                    .onKeyPress(.upArrow) {
                        highlighted = max(highlighted - 1, 0)
                        return .handled
                    }
                    .onKeyPress(.escape) {
                        store.showsBranches = false
                        return .handled
                    }
            }
            .padding(.horizontal, 16)
            .frame(height: Platform.scale > 1 ? 52 : 38)
            ThemeDivider()
            list(choices)
            let note: String? = working ? "An agent is working in this project. Switch when it has finished." : problem
            if let note {
                ThemeDivider()
                Text(note)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(working ? Color.themeSecondary : Color.themeDanger)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            }
        }
        #if os(macOS)
        .frame(width: 300)
        .onAppear {
            DispatchQueue.main.async { searching = true }
        }
        #else
        // A screen of the thread's settings, where the keyboard only comes when the field is tapped.
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Color.themeSheet)
        .navigationTitle(base == nil ? "Branch" : "Start from")
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .onChange(of: query) { highlighted = 0 }
    }

    @ViewBuilder private func list(_ choices: [Choice]) -> some View {
        if let listProblem {
            Text(listProblem)
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeDanger)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
        } else if choices.isEmpty {
            Text("No branch matches.")
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(Array(choices.enumerated()), id: \.element.id) { index, choice in
                            row(choice, index: index)
                                .button(.highlight(radius: 8)) { choose(choices, at: index) }
                                .onHover { if $0 { highlighted = index } }
                                .id(choice.id)
                        }
                    }
                    .padding(Self.listPadding)
                }
                #if os(macOS)
                .frame(height: listHeight)
                #endif
                .onChange(of: highlighted) {
                    guard choices.indices.contains(highlighted) else { return }
                    proxy.scrollTo(choices[highlighted].id)
                }
            }
        }
    }

    private func row(_ choice: Choice, index: Int) -> some View {
        HStack(spacing: 7) {
            switch choice {
            case .branch(let branch):
                let chosen = base.map { $0 == branch.name } ?? branch.current
                Image(chosen ? .check : .gitBranch, size: 11)
                    .foregroundStyle(chosen ? Color.themeText : Color.themeTertiary)
                    .frame(width: 14)
                Text(branch.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                let tag: String? = branch.isDefault ? "default" : branch.remote ? "remote" : nil
                if let tag {
                    Text(tag)
                        .font(.ui(size: 11))
                        .foregroundStyle(Color.themeTertiary)
                }
            case .create(let name):
                Image(.plus, size: 11)
                    .foregroundStyle(Color.themeTertiary)
                    .frame(width: 14)
                Text("Create branch “\(name)”")
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
            }
        }
        .font(.ui(size: 12.5))
        .foregroundStyle(Color.themeText)
        .padding(.horizontal, 8)
        .frame(height: Self.rowHeight)
        .frame(maxWidth: .infinity)
        .background(index == highlighted ? Color.themeHover : Color.clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .opacity(working ? 0.5 : 1)
    }

    private func choose(_ choices: [Choice], at index: Int) {
        guard !switching, !working, choices.indices.contains(index) else { return }
        let (name, create) = switch choices[index] {
        case .branch(let branch): (branch.name, false)
        case .create(let name): (name, true)
        }
        if base != nil {
            store.setDraftBase(name)
            store.showsBranches = false
            return
        }
        if !create, branches?.first(where: { $0.name == name })?.current == true {
            store.showsBranches = false
            return
        }
        switching = true
        problem = nil
        store.switchBranch(of: project, to: name, create: create) { failure in
            switching = false
            guard let failure else {
                store.showsBranches = false
                return
            }
            problem = failure
        }
    }
}
