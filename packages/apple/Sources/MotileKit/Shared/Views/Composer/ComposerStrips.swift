import SwiftUI

/// A strip against the composer's top or bottom, on the composer's surface: rounded on
/// its outer corners and open where it meets the composer. It stands in from the composer's
/// sides by the composer's corner radius, so it meets the composer's straight edge.
struct ComposerStrip: ViewModifier {
    enum Edge {
        case top, bottom
    }

    static let radius = Radius.lg
    /// The room between a control and the strip's visible edges.
    static let inset: CGFloat = 3
    static let height = ControlSize.regular.height + inset * 2
    /// The room around a control in a strip, which is the control's to click.
    static let margin = EdgeInsets(top: inset, leading: 4, bottom: inset, trailing: 8)

    let edge: Edge
    /// Nil for a strip that is as tall as what is in it.
    var height: CGFloat? = Self.height

    func body(content: Content) -> some View {
        let hidden: SwiftUI.Edge.Set = edge == .top ? .bottom : .top
        content
            .padding(hidden, Self.overlap)
            .frame(height: height.map { $0 + Self.overlap })
            .composerSurface(in: StripShape(edge: edge))
            .padding(.horizontal, ComposerView.radius)
            .padding(hidden, -Self.overlap)
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
                part(server.shortName) {
                    Image(.server, size: 11)
                }
                .padding(.leading, 14)
                .help("On \(server.name)")
                ComposerDivider()
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
                ComposerDivider()
            }
            branchPart
                .padding(.trailing, ComposerStrip.inset - ComposerStrip.margin.trailing)
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
                branchLabel(branch)
                    .help("The branch of this thread's worktree")
            } else if store.canSwitchBranches(of: project) {
                ActionButton(
                    startsInWorktree ? "From \(store.draftStart ?? branch)" : branch, icon: .gitBranch,
                    help: startsInWorktree ? startHelp(base: branch) : "Switch the branch of \(project.name)",
                    variant: .ghost, opens: true, margin: ComposerStrip.margin
                ) {
                    store.showBranches(of: project)
                }
                .truncationMode(.middle)
                .layoutPriority(1)
                .popover(isPresented: $store.showsBranches, arrowEdge: .bottom) {
                    BranchPicker(project: project, base: startsInWorktree ? branch : nil)
                }
            } else {
                branchLabel(branch)
                    .help(server?.known == true ? "Update \(server?.name ?? "your server") to switch branches from here" : "The branch checked out there")
            }
        }
    }

    private func startHelp(base: String) -> String {
        guard let start = store.draftStart, start != base else { return "The branch the worktree's branch starts from" }
        return "The worktree starts from \(start), which has commits \(base) doesn't have here yet"
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
            let margin = EdgeInsets(top: ComposerStrip.inset, leading: 4, bottom: ComposerStrip.inset, trailing: 4)
            ActionMenu(
                inWorktree ? "New worktree" : "Current checkout", icon: inWorktree ? .folderGit2 : .folder,
                help: inWorktree ? "The thread works in a folder and on a branch of its own" : "The thread works in the project's folder",
                margin: margin
            ) {
                Section("Workspace") {
                    Toggle(isOn: Binding(get: { !inWorktree }, set: { _ in store.setDraftWorktree(false) })) {
                        Label("Current checkout", symbol: .folder)
                    }
                    Toggle(isOn: Binding(get: { inWorktree }, set: { _ in store.setDraftWorktree(true) })) {
                        Label("New worktree", symbol: .folderGit2)
                    }
                }
            }
        }
    }

    private func working(in title: String, symbol: Symbol) -> some View {
        part(title) {
            Image(symbol, size: 11)
        }
        .padding(.horizontal, 13)
    }

    private func part(_ title: String, @ViewBuilder icon: () -> some View) -> some View {
        HStack(spacing: 5) {
            icon()
            Text(title)
                .font(.ui(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .foregroundStyle(Color.themeMutedForeground)
    }

    /// A branch that can't be switched from here, set as the button that switches one is.
    private func branchLabel(_ branch: String) -> some View {
        ControlLabel(title: branch, icon: .symbol(.gitBranch), size: .regular)
            .truncationMode(.middle)
            .foregroundStyle(Color.themeMutedForeground)
            .padding(ComposerStrip.margin)
    }
}
#endif

/// The line between the parts of the composer's rows. It takes no room and no clicks, so the
/// controls on either side of it reach its middle.
struct ComposerDivider: View {
    var body: some View {
        Color.clear
            .frame(width: 0, height: 14)
            .overlay {
                Rectangle()
                    .fill(Color.themeBorderInput)
                    .frame(width: 1)
            }
            .allowsHitTesting(false)
    }
}

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
    private static let placeholderCount = 5

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

    private var prompt: String { base == nil ? "Switch or create a branch" : "Start from a branch" }

    private var branches: [Branch]? { try? store.listedBranches?.get() }

    /// The height of all the branches, also while fewer match: a popover that shrinks leaves
    /// the button it opened from.
    private var listHeight: CGFloat {
        let rows = store.listedBranches == nil ? Self.placeholderCount : branches?.count ?? 0
        return min(Self.maxListHeight, CGFloat(rows) * Self.rowHeight + 2 * Self.listPadding)
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
            #if os(macOS)
            HStack(spacing: 8) {
                Image(.search, size: 12)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                TextField("", text: $query, prompt: .placeholder(prompt))
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
            .frame(height: 38)
            ThemeDivider()
            #endif
            list(choices)
            let note: String? = working ? "An agent is working in this project. Switch when it has finished." : problem
            if let note {
                ThemeDivider()
                Text(note)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(working ? Color.themeMutedForeground : Color.themeDestructive)
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
        .background(Color.themeBackground)
        .environment(\.surface, .background)
        .navigationTitle(base == nil ? "Branch" : "Start From")
        .navigationBarTitleDisplayMode(.inline)
        .searchField(text: $query, prompt: prompt)
        .searchPresentationToolbarBehavior(.avoidHidingContent)
        .searchAtBottom()
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
        .onSubmit(of: .search) { choose(choices, at: highlighted) }
        #endif
        .onChange(of: query) { highlighted = 0 }
    }

    @ViewBuilder private func list(_ choices: [Choice]) -> some View {
        if store.listedBranches == nil {
            VStack(spacing: 0) {
                ForEach(0..<Self.placeholderCount, id: \.self) { index in
                    BranchPlaceholder(index: index, height: Self.rowHeight)
                }
            }
            .padding(Self.listPadding)
            #if os(macOS)
            .frame(height: listHeight, alignment: .top)
            #endif
        } else if let listProblem {
            Text(listProblem)
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeDestructive)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
        } else if choices.isEmpty {
            Text("No branch matches.")
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeMutedForeground)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(Array(choices.enumerated()), id: \.element.id) { index, choice in
                            row(choice, index: index)
                                .button(.highlight(radius: Radius.md, lit: index == highlighted)) { choose(choices, at: index) }
                                .onHover { if $0 { highlighted = index } }
                                .id(choice.id)
                        }
                    }
                    .padding(Self.listPadding)
                    .padding(.bottom, Platform.scale > 1 ? 24 : 0)
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
                    .foregroundStyle(chosen ? Color.themeForeground : Color.themeMutedStrongerForeground)
                    .frame(width: 14)
                Text(branch.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                let tag: String? = base != nil && branch.current ? "current"
                    : branch.isDefault ? "default" : branch.remote ? "remote" : nil
                if let tag {
                    Text(tag)
                        .font(.ui(size: 11))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                }
            case .create(let name):
                Image(.plus, size: 11)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .frame(width: 14)
                Text("Create branch “\(name)”")
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
            }
        }
        .font(.ui(size: 12.5))
        .foregroundStyle(Color.themeForeground)
        .padding(.horizontal, 8)
        .frame(height: Self.rowHeight)
        .frame(maxWidth: .infinity)
        .opacity(.disabled, when: working)
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

/// A branch's row before the branches are listed: bars where its icon and name will be.
private struct BranchPlaceholder: View {
    @Environment(\.surface) private var surface
    let index: Int
    let height: CGFloat
    @State private var faded = false

    private static let widths: [CGFloat] = [90, 150, 120, 170, 105]

    var body: some View {
        HStack(spacing: 7) {
            RoundedRectangle(cornerRadius: Radius.xs, style: .continuous)
                .fill(surface.color(.control))
                .frame(width: 11, height: 11)
                .frame(width: 14)
            Capsule()
                .fill(surface.color(.control))
                .frame(width: Self.widths[index % Self.widths.count], height: 8)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 8)
        .frame(height: height)
        .opacity(.disabled, when: faded)
        .animation(.easeInOut(duration: 0.8).repeatForever(autoreverses: true), value: faded)
        .onAppear { faded = true }
    }
}
