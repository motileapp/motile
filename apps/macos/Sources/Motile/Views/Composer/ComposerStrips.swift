import SwiftUI

/// A strip against the composer's top or bottom, in the composer's fill and border: rounded on
/// its outer corners and open where it meets the composer. It stands in from the composer's
/// sides by the composer's corner radius, so it meets the composer's straight edge.
struct ComposerStrip: ViewModifier {
    enum Edge {
        case top, bottom
    }

    static let height: CGFloat = 32
    static let radius: CGFloat = 14
    /// The room around a control in a strip, which is the control's to click.
    static let margin = EdgeInsets(top: 4, leading: 4, bottom: 4, trailing: 8)

    let edge: Edge

    func body(content: Content) -> some View {
        content
            .frame(height: Self.height)
            .background { StripShape(edge: edge, closed: true).fill(Color.themeComposer) }
            .overlay { StripShape(edge: edge, closed: false).stroke(Color.themeStrongBorder, lineWidth: 1) }
            .padding(.horizontal, ComposerView.radius)
    }
}

/// The strip's outline: three sides, rounded on the two outer corners. Closed, it is the fill.
private struct StripShape: Shape {
    let edge: ComposerStrip.Edge
    let closed: Bool

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
        if closed { path.closeSubpath() }
        return path
    }
}

/// Where the composer's thread works: the server, the folder or a worktree of its own, and the
/// branch checked out there, which opens the picker to switch. A thread that starts in a new
/// worktree picks the branch it starts from there instead.
struct ContextStrip: View {
    @Environment(AppStore.self) private var store
    let project: Project
    let server: Server?

    var body: some View {
        @Bindable var store = store
        HStack(spacing: 0) {
            if let server {
                part(server.name) {
                    Image(systemName: "server.rack")
                        .font(.system(size: 11, weight: .medium))
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
            workspace
            Spacer(minLength: 8)
            let startsInWorktree = store.draftUsesWorktree
            if let branch = startsInWorktree ? store.draftBase : project.branch {
                if project.worktree != nil {
                    branchLabel(branch, opens: false)
                        .help("The branch of this thread's worktree")
                } else if store.canSwitchBranches(of: project) {
                    Button {
                        store.showBranches(of: project)
                    } label: {
                        branchLabel(startsInWorktree ? "From \(branch)" : branch, opens: true)
                    }
                    .buttonStyle(.plain)
                    .hoverHighlight(radius: 7, inset: ComposerStrip.margin)
                    .help(startsInWorktree ? "The branch the worktree's branch starts from" : "Switch the branch of \(project.name)")
                    .popover(isPresented: $store.showsBranches, arrowEdge: .bottom) {
                        BranchPicker(project: project, base: startsInWorktree ? branch : nil)
                    }
                } else {
                    branchLabel(branch, opens: false)
                        .help(server?.known == true ? "Update \(server?.name ?? "your server") to switch branches from here" : "The branch checked out there")
                }
            }
        }
        .modifier(ComposerStrip(edge: .bottom))
    }

    /// Where a new thread starts, to choose, and where a thread that has started works.
    @ViewBuilder private var workspace: some View {
        if let worktree = project.worktree {
            working(in: "Worktree", symbol: "folder.badge.gearshape")
                .help(worktree.path)
        } else if store.selectedThread != nil, project.branch != nil {
            working(in: "Local checkout", symbol: "folder")
                .help("The thread works in the project's folder")
        } else if store.selectedThread == nil, store.canUseWorktrees(of: project) {
            let inWorktree = store.draftUsesWorktree
            let margin = EdgeInsets(top: 4, leading: 3, bottom: 4, trailing: 3)
            divider
                .padding(.leading, 10)
            Menu {
                Section("Workspace") {
                    Toggle(isOn: Binding(get: { !inWorktree }, set: { _ in store.setDraftWorktree(false) })) {
                        Label("Current checkout", systemImage: "folder")
                    }
                    Toggle(isOn: Binding(get: { inWorktree }, set: { _ in store.setDraftWorktree(true) })) {
                        Label("New worktree", systemImage: "folder.badge.plus")
                    }
                }
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: inWorktree ? "folder.badge.plus" : "folder")
                        .font(.system(size: 11, weight: .medium))
                    Text(inWorktree ? "New worktree" : "Current checkout")
                        .font(.system(size: 12))
                        .lineLimit(1)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(Color.themeTertiary)
                }
                .foregroundStyle(Color.themeSecondary)
                .padding(.horizontal, 9)
                .frame(height: 24)
                .padding(margin)
                .contentShape(Rectangle())
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .menuIndicator(.hidden)
            .fixedSize()
            .hoverHighlight(radius: 7, inset: margin)
            .help(inWorktree ? "The thread works in a folder and on a branch of its own" : "The thread works in the project's folder")
        }
    }

    @ViewBuilder private func working(in title: String, symbol: String) -> some View {
        divider
            .padding(.horizontal, 10)
        part(title) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .medium))
        }
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
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .foregroundStyle(Color.themeSecondary)
    }

    private func branchLabel(_ branch: String, opens: Bool) -> some View {
        HStack(spacing: 5) {
            Image(systemName: "arrow.triangle.branch")
                .font(.system(size: 11, weight: .medium))
            Text(branch)
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)
            if opens {
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Color.themeTertiary)
            }
        }
        .foregroundStyle(Color.themeSecondary)
        .padding(.horizontal, 9)
        .frame(height: 24)
        .padding(ComposerStrip.margin)
        .contentShape(Rectangle())
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

    private static let rowHeight: CGFloat = 30
    private static let listPadding: CGFloat = 8
    private static let maxListHeight: CGFloat = 300

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
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(Color.themeTertiary)
                TextField(base == nil ? "Switch or create a branch…" : "Start from a branch…", text: $query)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13))
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
            Divider()
            list(choices)
            let note: String? = working ? "An agent is working in this project. Switch when it has finished." : problem
            if let note {
                Divider()
                Text(note)
                    .font(.system(size: 11.5))
                    .foregroundStyle(working ? Color.themeSecondary : Color.themeDanger)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
            }
        }
        .frame(width: 300)
        .onAppear {
            DispatchQueue.main.async { searching = true }
        }
        .onChange(of: query) { highlighted = 0 }
    }

    @ViewBuilder private func list(_ choices: [Choice]) -> some View {
        if let listProblem {
            Text(listProblem)
                .font(.system(size: 12.5))
                .foregroundStyle(Color.themeDanger)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
        } else if choices.isEmpty {
            Text("No branch matches.")
                .font(.system(size: 12.5))
                .foregroundStyle(Color.themeSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(Array(choices.enumerated()), id: \.element.id) { index, choice in
                            row(choice, index: index)
                                .onTapGesture { choose(choices, at: index) }
                                .onHover { if $0 { highlighted = index } }
                                .id(choice.id)
                        }
                    }
                    .padding(Self.listPadding)
                }
                .frame(height: listHeight)
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
                Image(systemName: chosen ? "checkmark" : "arrow.triangle.branch")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(chosen ? Color.themeText : Color.themeTertiary)
                    .frame(width: 14)
                Text(branch.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                let tag: String? = branch.isDefault ? "default" : branch.remote ? "remote" : nil
                if let tag {
                    Text(tag)
                        .font(.system(size: 11))
                        .foregroundStyle(Color.themeTertiary)
                }
            case .create(let name):
                Image(systemName: "plus")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(Color.themeTertiary)
                    .frame(width: 14)
                Text("Create branch “\(name)”")
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
            }
        }
        .font(.system(size: 12.5))
        .foregroundStyle(Color.themeText)
        .padding(.horizontal, 8)
        .frame(height: Self.rowHeight)
        .frame(maxWidth: .infinity)
        .background(index == highlighted ? Color.themeHover : Color.clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .contentShape(Rectangle())
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
