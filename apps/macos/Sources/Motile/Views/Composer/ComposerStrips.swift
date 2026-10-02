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

/// Where the composer's thread works: the server, the folder, and the branch checked out there,
/// which opens the picker to switch.
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
                Rectangle()
                    .fill(Color.themeBorder)
                    .frame(width: 1, height: 14)
                    .padding(.horizontal, 10)
            }
            part(project.name) {
                ProjectIcon(project: project, size: 13)
            }
            .padding(.leading, server == nil ? 14 : 0)
            .help(project.path)
            Spacer(minLength: 8)
            if let branch = project.branch {
                if store.canSwitchBranches(of: project) {
                    Button {
                        store.showsBranches = true
                    } label: {
                        branchLabel(branch, opens: true)
                    }
                    .buttonStyle(.plain)
                    .hoverHighlight(radius: 7, inset: ComposerStrip.margin)
                    .help("Switch the branch of \(project.name)")
                    .popover(isPresented: $store.showsBranches, arrowEdge: .bottom) {
                        BranchPicker(project: project)
                    }
                } else {
                    branchLabel(branch, opens: false)
                        .help(server?.known == true ? "Update \(server?.name ?? "your server") to switch branches from here" : "The branch checked out there")
                }
            }
        }
        .modifier(ComposerStrip(edge: .bottom))
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
/// narrows the list, and becomes the name of a branch to make when it matches none.
struct BranchPicker: View {
    @Environment(AppStore.self) private var store
    let project: Project
    @State private var query = ""
    @State private var branches: [Branch]?
    @State private var problem: String?
    @State private var highlighted = 0
    @State private var switching = false
    @FocusState private var searching: Bool

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

    private var working: Bool { store.isWorking(in: project) }

    private var choices: [Choice] {
        guard let branches else { return [] }
        let needle = query.trimmingCharacters(in: .whitespaces)
        let matching = branches.filter { needle.isEmpty || $0.name.localizedCaseInsensitiveContains(needle) }
        var choices = matching.map(Choice.branch)
        if !needle.isEmpty, !branches.contains(where: { $0.name == needle }) {
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
                TextField("Switch or create a branch…", text: $query)
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
            .padding(.horizontal, 12)
            .frame(height: 38)
            Divider()
            list(choices)
            let note: String? = working ? "An agent is working in this project. Switch when it has finished." : problem
            if let note {
                Divider()
                Text(note)
                    .font(.system(size: 11.5))
                    .foregroundStyle(working ? Color.themeSecondary : Color.themeDanger)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            }
        }
        .frame(width: 300)
        .onAppear {
            DispatchQueue.main.async { searching = true }
            load()
        }
        .onChange(of: query) { highlighted = 0 }
    }

    @ViewBuilder private func list(_ choices: [Choice]) -> some View {
        if branches == nil {
            Text(problem ?? "Loading…")
                .font(.system(size: 12.5))
                .foregroundStyle(problem == nil ? Color.themeSecondary : Color.themeDanger)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
        } else if choices.isEmpty {
            Text("No branch matches.")
                .font(.system(size: 12.5))
                .foregroundStyle(Color.themeSecondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(Array(choices.enumerated()), id: \.element.id) { index, choice in
                            row(choice, index: index)
                                .onTapGesture { choose(choices, at: index) }
                                .onHover { if $0 { highlighted = index } }
                                .id(index)
                        }
                    }
                    .padding(4)
                }
                .frame(maxHeight: 300)
                .onChange(of: highlighted) { proxy.scrollTo(highlighted) }
            }
        }
    }

    private func row(_ choice: Choice, index: Int) -> some View {
        HStack(spacing: 7) {
            switch choice {
            case .branch(let branch):
                Image(systemName: branch.current ? "checkmark" : "arrow.triangle.branch")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(branch.current ? Color.themeText : Color.themeTertiary)
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
        .frame(height: 28)
        .frame(maxWidth: .infinity)
        .background(index == highlighted ? Color.themeSelected : Color.clear, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
        .contentShape(Rectangle())
        .opacity(working ? 0.5 : 1)
    }

    private func load() {
        store.listBranches(of: project) { result in
            switch result {
            case .success(let listed): branches = listed
            case .failure(let error): problem = error.message
            }
        }
    }

    private func choose(_ choices: [Choice], at index: Int) {
        guard !switching, !working, choices.indices.contains(index) else { return }
        let (name, create) = switch choices[index] {
        case .branch(let branch): (branch.name, false)
        case .create(let name): (name, true)
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
