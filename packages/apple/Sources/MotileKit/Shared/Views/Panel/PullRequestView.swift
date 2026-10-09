import SwiftUI

/// A pull request of the repository: the one of the branch the thread works on, or another one by
/// its number. Where it stands, what holds it up, the button its state calls for, what was said on
/// it, and a box to say more.
struct PullRequestSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    /// Another pull request than the thread's own.
    var number: Int?
    @State private var asked = 0

    private var shown: Int? { number ?? target.pullRequest }

    var body: some View {
        let panel = store.sidePanel
        VStack(spacing: 0) {
            PanelBar { bar(panel.pullRequest.value.flatMap { $0.number == shown ? $0 : nil }) }
            if let notice = panel.pullRequestNotice {
                PullRequestNoticeBar(notice: notice)
            }
            if let reason = store.pullRequestsUnavailable {
                PanelMessage(text: reason)
            } else if let number = shown {
                content(number)
                    .panelTask(id: PanelTrigger(target: target, path: String(number), version: store.workspaceVersion, asked: asked)) {
                        panel.loadPullRequest(of: target, number: number)
                    }
                    .panelTask(id: panel.pullRequestReads) {
                        // While GitHub is still working something out, it is asked again.
                        guard panel.pullRequest.value?.settling == true else { return }
                        try? await Task.sleep(for: .seconds(10))
                        guard !Task.isCancelled else { return }
                        panel.loadPullRequest(of: target, number: number)
                    }
            } else {
                NoPullRequest(target: target)
            }
        }
    }

    @ViewBuilder
    private func bar(_ page: PullRequestPage?) -> some View {
        if let page {
            Image(page.state.symbol, size: 12)
                .foregroundStyle(page.state.color)
            Text(verbatim: "#\(page.number)")
                .font(.ui(size: 12.5, weight: .medium))
                .foregroundStyle(Color.themeForeground)
                .monospacedDigit()
                .padding(.leading, 2)
            if page.number == target.pullRequest, store.selectedThread?.watching == true {
                Image(.eye, size: 12)
                    .foregroundStyle(Color.themePrimary)
                    .padding(.leading, 4)
                    .help("The agent hears when its checks finish, someone comments or it conflicts")
            }
        } else {
            Text("Pull Request")
                .font(.ui(size: 12.5, weight: .medium))
                .foregroundStyle(Color.themeForeground)
        }
        Spacer(minLength: 4)
        if let page, let url = page.url {
            ActionButton(icon: .squareArrowOutUpRight, help: "Open on GitHub") { Platform.open(url) }
        }
        if shown != nil, store.pullRequestsUnavailable == nil {
            ActionButton(icon: .rotateCw, help: "Read the pull request again") { asked += 1 }
        }
    }

    @ViewBuilder
    private func content(_ number: Int) -> some View {
        let panel = store.sidePanel
        switch panel.pullRequest {
        case .ready(let page) where page.number == number:
            PullRequestPageView(page: page, target: target, number: number)
        case .failed(let message):
            PanelMessage(text: message, failed: true)
        default:
            PanelLoading()
        }
    }
}

/// The thread's tab while its branch has no pull request: the way to open one, or to link one.
private struct NoPullRequest: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    @State private var linking = ""

    var body: some View {
        VStack(spacing: 14) {
            Image(.gitPullRequest, size: 22)
                .foregroundStyle(Color.themeMutedStrongerForeground)
            Text("This branch has no pull request yet.")
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeMutedForeground)
            HStack(spacing: 8) {
                if let project = store.gitProject, let create = project.gitControl?.menu.first(where: { $0.action == "create_pr" }),
                    create.reason == nil
                {
                    ActionButton("Create PR", variant: .primary) { store.chooseGit(create, in: project) }
                }
                if store.pullRequestsExtended {
                    ActionButton("Show All Pull Requests") { store.sidePanel.open(.pullRequests) }
                }
            }
            if store.pullRequestsExtended, let thread = store.selectedThread {
                LinkPullRequestField(text: $linking) { number in
                    store.sidePanel.link(number, thread: thread.id, serverID: thread.serverID)
                }
                .frame(maxWidth: 300)
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// A field that takes a pull request's number or address and links it to the thread.
struct LinkPullRequestField: View {
    @Binding var text: String
    let link: (Int) -> Void

    /// The number in "#12", "12" or "https://github.com/acme/app/pull/12".
    private var number: Int? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let last = trimmed.split(separator: "/").last.map(String.init) ?? trimmed
        return Int(last.hasPrefix("#") ? String(last.dropFirst()) : last)
    }

    var body: some View {
        HStack(spacing: 6) {
            InputField("Link a PR by number or address", text: $text)
                .onSubmit(submit)
            ActionButton("Link", action: submit)
                .disabled(number == nil)
        }
    }

    private func submit() {
        guard let number else { return }
        link(number)
        text = ""
    }
}

/// What the tab can do to the pull request, for the parts of the page that do it.
struct PullRequestActions {
    let react: (_ subject: String, _ kind: String, _ on: Bool) -> Void
    let reply: (_ thread: String, _ body: String, _ done: @escaping () -> Void) -> Void
    let resolve: (_ thread: String, _ resolved: Bool) -> Void
    let handOff: (String) -> Void
    let showCommit: (String) -> Void
    let editDescription: (() -> Void)?
    /// The action that runs.
    let working: PullRequestWork?
}

private struct PullRequestPageView: View {
    @Environment(AppStore.self) private var store
    let page: PullRequestPage
    let target: PanelTarget
    let number: Int
    @State private var confirming: PullRequestButton?
    @State private var comment = ""
    @State private var title: String?
    @FocusState private var titleFocused: Bool
    @State private var describing = false

    private var panel: SidePanel { store.sidePanel }
    private var extended: Bool { store.pullRequestsExtended }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                if extended, let stack = page.stack, stack.layers.count > 1 {
                    StackCard(stack: stack) { panel.showPullRequest($0, of: target) }
                }
                MergeBox(page: page, working: panel.pullRequestWorking, run: run)
                VStack(alignment: .leading, spacing: 16) {
                    activityTitle
                    VStack(alignment: .leading, spacing: 20) {
                        ForEach(page.activity) { entry in
                            ActivityRow(entry: entry, actions: actions)
                        }
                        if page.state != .merged {
                            commentBox
                        }
                    }
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .alert(
            confirming?.confirm?.title ?? "", isPresented: confirmingShown, presenting: confirming
        ) { button in
            Button(button.confirm?.button ?? button.label, role: button.style == "danger" ? .destructive : nil) { perform(button) }
            Button("Cancel", role: .cancel) {}
        } message: { button in
            Text(button.confirm?.message ?? "")
        }
        .sheet(isPresented: $describing) {
            DescriptionEditor(original: page.body) { body in
                panel.edit(["kind": "body", "body": body], key: "menu:body", on: target, number: number)
            }
            .sheetSurface()
        }
    }

    private var actions: PullRequestActions {
        PullRequestActions(
            react: { subject, kind, on in
                panel.edit(["kind": "react", "subject": subject, "reaction": kind, "on": on], on: target, number: number)
            },
            reply: { thread, body, done in
                panel.edit(["kind": "reply", "thread": thread, "body": body], key: "reply:\(thread)", on: target, number: number, done: done)
            },
            resolve: { thread, resolved in
                panel.edit(["kind": "resolve", "thread": thread, "resolved": resolved], key: "resolve:\(thread)", on: target, number: number)
            },
            handOff: { store.handOff($0) },
            showCommit: { panel.showDiff(.commit($0)) },
            editDescription: extended && page.canEdit ? { describing = true } : nil,
            working: panel.pullRequestWorking)
    }

    // MARK: Header

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                if let editing = title {
                    titleEditor(editing)
                } else {
                    Text(page.title)
                        .font(.ui(size: 15, weight: .semibold))
                        .foregroundStyle(Color.themeForeground)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                    moreMenu
                        .padding(.top, -4)
                }
            }
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Chip(page.state.title, icon: page.state.symbol, tone: page.state.color)
                Text(page.byline)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 6) {
                BranchName(name: page.base)
                Image(.arrowLeft, size: 10)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                BranchName(name: page.head)
                Spacer(minLength: 8)
                Button {
                    panel.showDiff(.pullRequest(page.number))
                } label: {
                    HStack(spacing: 6) {
                        Image(.diff, size: 11)
                            .foregroundStyle(Color.themeMutedForeground)
                        Text(page.files == 1 ? "1 file" : "\(page.files) files")
                            .foregroundStyle(Color.themeForeground)
                        if page.additions + page.deletions > 0 {
                            Text(AttributedString(LineCountText.text(added: page.additions, removed: page.deletions)))
                        }
                    }
                    .font(.ui(size: 12))
                    .padding(.horizontal, 6)
                    .frame(height: pressable(24))
                }
                .buttonStyle(.highlight(radius: Radius.sm))
                .help("Show what it changes")
            }
            if extended, let below = page.stackedOn {
                Button {
                    panel.showPullRequest(below.number, of: target)
                } label: {
                    HStack(spacing: 6) {
                        Image(.layers, size: 11)
                            .foregroundStyle(Color.themeMutedForeground)
                        (Text("Stacked on ").foregroundStyle(Color.themeMutedForeground)
                            + Text(verbatim: "#\(below.number) ").foregroundStyle(below.state.color)
                            + Text(below.title).foregroundStyle(Color.themeForeground))
                            .lineLimit(1)
                    }
                    .font(.ui(size: 12))
                    .padding(.horizontal, 6)
                    .frame(height: pressable(24))
                }
                .buttonStyle(.highlight(radius: Radius.sm))
                .padding(.leading, -6)
                .help("Open the pull request it merges into")
            }
            if extended {
                people
            }
        }
    }

    private func titleEditor(_ editing: String) -> some View {
        let binding = Binding { title ?? "" } set: { title = $0 }
        let working = panel.pullRequestWorking?.key == "title"
        return VStack(alignment: .leading, spacing: 8) {
            InputField("Title", text: binding, size: .large, focus: $titleFocused)
                .onSubmit(saveTitle)
                .onAppear { titleFocused = true }
                .onEscape { title = nil }
            HStack(spacing: 8) {
                Spacer()
                ActionButton("Cancel") { title = nil }
                    .disabled(working)
                ActionButton("Save", variant: .primary, pending: working, action: saveTitle)
                    .disabled(working || editing.trimmingCharacters(in: .whitespaces).isEmpty || editing == page.title)
            }
        }
    }

    private func saveTitle() {
        guard let edited = title?.trimmingCharacters(in: .whitespaces), !edited.isEmpty, edited != page.title else { return }
        panel.edit(["kind": "title", "title": edited], key: "title", on: target, number: number) { title = nil }
    }

    /// Who reviews it and its labels, each with a menu to change them when the user may.
    @ViewBuilder
    private var people: some View {
        let working = panel.pullRequestWorking
        let reviews = !page.reviewers.isEmpty || !page.reviewerChoices.isEmpty
        let labelled = !page.labels.isEmpty || !page.labelChoices.isEmpty
        if reviews || labelled {
            VStack(alignment: .leading, spacing: 6) {
                if reviews {
                    PeopleRow(title: "Reviewers", empty: page.reviewers.isEmpty ? "None yet" : nil, choices: page.reviewerChoices, help: "Ask for a review", pending: working?.key == "menu:reviewers") {
                        ForEach(page.reviewers) { ReviewerChip(reviewer: $0) }
                    } toggle: { name, on in
                        let edit: JSON = ["kind": "reviewers", "add": on ? [name] : [], "remove": on ? [] : [name]]
                        panel.edit(edit, key: "menu:reviewers", on: target, number: number)
                    }
                }
                if labelled {
                    PeopleRow(title: "Labels", empty: page.labels.isEmpty ? "None yet" : nil, choices: page.labelChoices, help: "Change the labels", pending: working?.key == "menu:labels") {
                        ForEach(page.labels, id: \.name) { Chip($0.name, dot: $0.color, tone: $0.color) }
                    } toggle: { name, on in
                        let edit: JSON = ["kind": "labels", "add": on ? [name] : [], "remove": on ? [] : [name]]
                        panel.edit(edit, key: "menu:labels", on: target, number: number)
                    }
                }
            }
            .padding(.top, 2)
        }
    }

    /// Everything else there is to do, after what the merge box offers.
    private var moreMenu: some View {
        let work = panel.pullRequestWorking
        let working = work != nil
        let thread = store.selectedThread
        let ownPullRequest = thread?.pullRequest?.number == number
        // What a menu started has no button of its own to be pending.
        let pending = work.map { $0.key.hasPrefix("menu:") && !["menu:reviewers", "menu:labels"].contains($0.key) } ?? false
        return ActionMenu(icon: .ellipsis, help: "More", pending: pending) {
            ForEach(page.menu) { button in
                Button(role: button.style == "danger" ? .destructive : nil) { run(button, fromMenu: true) } label: { Text(button.label) }
                    .disabled(working && button.prompt == nil)
            }
            if extended {
                Divider()
                if page.canEdit {
                    Button("Edit Title") { title = page.title }
                    Button("Edit Description") { describing = true }
                }
                if let thread {
                    if ownPullRequest {
                        if page.watchable {
                            Button(thread.watching ? "Stop Watching" : "Watch for Changes") {
                                panel.watch(!thread.watching, thread: thread.id, serverID: thread.serverID)
                            }
                        }
                        Button("Unlink from This Thread") { panel.link(nil, thread: thread.id, serverID: thread.serverID) }
                    } else {
                        Button("Link to This Thread") { panel.link(number, thread: thread.id, serverID: thread.serverID) }
                    }
                }
                Button("Show All Pull Requests") { panel.open(.pullRequests) }
            }
            Divider()
            if let url = page.url {
                Button("Copy Link") { Platform.copy(url.absoluteString) }
            }
            Button("Copy Branch Name") { Platform.copy(page.head) }
        }
    }

    /// Parts the merge box, which is where the pull request stands now, from what happened on it.
    private var activityTitle: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text("Activity")
                .font(.ui(size: 13, weight: .semibold))
                .foregroundStyle(Color.themeForeground)
            Text("Oldest first")
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeMutedStrongerForeground)
        }
        .padding(.top, 6)
    }

    // MARK: Comment box

    private var commentBox: some View {
        let work = panel.pullRequestWorking
        let busy = work != nil
        let written = comment.trimmingCharacters(in: .whitespacesAndNewlines)
        let pending = panel.pendingComments[number] ?? []
        return VStack(alignment: .leading, spacing: 8) {
            if !pending.isEmpty {
                PendingComments(comments: pending) { panel.removePending($0, from: number) }
            }
            TextArea(pending.isEmpty ? "Leave a comment" : "Say something with your review (optional)", text: $comment, lines: 4)
            HStack(spacing: 8) {
                if let close = page.withComment, pending.isEmpty {
                    ActionButton(close.label, pending: work?.key == "close") { choose(close, key: "close") }
                    .disabled(busy || written.isEmpty)
                }
                Spacer(minLength: 0)
                ForEach(page.verdicts) { verdict in
                    let key = verdict.action
                    ActionButton(verdict.label, pending: work?.key == key) {
                        if pending.isEmpty {
                            choose(verdict, key: key)
                        } else {
                            panel.review(key, body: written, key: key, on: target, number: number) { comment = "" }
                        }
                    }
                    .disabled(busy || (verdict.action == "request_changes" && written.isEmpty && pending.isEmpty))
                }
                ActionButton(pending.isEmpty ? "Comment" : "Send Review", variant: .primary, pending: work?.key == "comment") {
                    if pending.isEmpty {
                        panel.act("comment", text: written, key: "comment", on: target, number: number) { comment = "" }
                    } else {
                        panel.review("comment", body: written, key: "comment", on: target, number: number) { comment = "" }
                    }
                }
                .disabled(busy || (written.isEmpty && pending.isEmpty))
            }
        }
        .padding(.top, 4)
    }

    private func choose(_ choice: PullRequestChoice, key: String) {
        let text = comment.trimmingCharacters(in: .whitespacesAndNewlines)
        panel.act(choice.action, method: choice.method, text: text, key: key, on: target, number: number) {
            comment = ""
        }
    }

    // MARK: Running

    /// A button's prompt goes to the composer; its action runs, after asking when it says to.
    private func run(_ button: PullRequestButton) {
        run(button, fromMenu: false)
    }

    private func run(_ button: PullRequestButton, fromMenu: Bool) {
        if let prompt = button.prompt {
            store.handOff(prompt)
            return
        }
        guard button.confirm == nil else {
            confirming = fromMenu ? button.inMenu : button
            return
        }
        perform(fromMenu ? button.inMenu : button)
    }

    private func perform(_ button: PullRequestButton) {
        guard let action = button.action else { return }
        panel.act(action, method: button.method, key: button.key, on: target, number: number)
    }

    private var confirmingShown: Binding<Bool> {
        Binding { confirming != nil } set: { shown in
            if !shown { confirming = nil }
        }
    }
}

/// A row of who reviews or what labels, with a menu of the choices when there are any.
private struct PeopleRow<Chips: View>: View {
    let title: String
    /// Said in place of the chips when there are none.
    let empty: String?
    let choices: [PullRequestPage.Toggle]
    let help: String
    var pending = false
    @ViewBuilder let chips: Chips
    let toggle: (String, Bool) -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            Text(title)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .frame(width: 70, alignment: .leading)
            if let empty {
                Text(empty)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
            } else {
                FlowRow(spacing: 5) { chips }
            }
            if !choices.isEmpty {
                ActionMenu(icon: .plus, help: help, size: .small, pending: pending) {
                    ForEach(choices) { choice in
                        Toggle(choice.name, isOn: Binding { choice.on } set: { toggle(choice.name, $0) })
                    }
                }
            }
            Spacer(minLength: 0)
        }
    }
}

/// Lays its views out in rows, as many to a row as fit.
struct FlowRow: Layout {
    var spacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var (x, y, rowHeight, widest): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0, x + size.width > width {
                y += rowHeight + spacing
                x = 0
                rowHeight = 0
            }
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
            widest = max(widest, x - spacing)
        }
        return CGSize(width: min(widest, width), height: y + rowHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var (x, y, rowHeight): (CGFloat, CGFloat, CGFloat) = (bounds.minX, bounds.minY, 0)
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > bounds.minX, x + size.width > bounds.maxX {
                y += rowHeight + spacing
                x = bounds.minX
                rowHeight = 0
            }
            subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
    }
}

/// The comments on lines that wait for the review, each one to take back.
private struct PendingComments: View {
    let comments: [PendingLineComment]
    let remove: (PendingLineComment) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(comments.count == 1 ? "1 comment on a line goes with your review" : "\(comments.count) comments on lines go with your review")
                .font(.ui(size: 12, weight: .medium))
                .foregroundStyle(Color.themeMutedForeground)
                .padding(.horizontal, 12)
                .padding(.top, 9)
                .padding(.bottom, 4)
            ForEach(comments) { comment in
                HStack(alignment: .top, spacing: 8) {
                    Text(verbatim: "\(URL(fileURLWithPath: comment.path).lastPathComponent):\(comment.line)")
                        .font(.ui(size: 11.5, design: .monospaced))
                        .foregroundStyle(Color.themePrimary)
                    Text(comment.body)
                        .font(.ui(size: 12.5))
                        .foregroundStyle(Color.themeForeground)
                        .lineLimit(2)
                    Spacer(minLength: 4)
                    ActionButton(icon: .x, help: "Take it back", size: .small) { remove(comment) }
                        .padding(.vertical, -4)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 4)
            }
        }
        .padding(.bottom, 6)
        .box(in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
    }
}

/// The stack the pull request is in, the top first, each one to open.
private struct StackCard: View {
    let stack: (url: URL?, base: String, layers: [PullRequestPage.StackLayer])
    let open: (Int) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Image(.layers, size: 12)
                    .foregroundStyle(Color.themeMutedForeground)
                Text("Stack")
                    .font(.ui(size: 13, weight: .medium))
                    .foregroundStyle(Color.themeForeground)
                Text("\(stack.layers.count) pull requests onto \(stack.base)")
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                Spacer(minLength: 4)
                if let url = stack.url {
                    ActionButton("Open on GitHub", help: "A stack merges on GitHub, bottom first", variant: .link, size: .small) {
                        Platform.open(url)
                    }
                    .padding(.trailing, -8)
                }
            }
            .padding(.horizontal, 12)
            .frame(minHeight: ControlSize.small.height)
            .padding(.top, 4)
            ForEach(stack.layers.reversed()) { layer in
                Button {
                    open(layer.number)
                } label: {
                    HStack(spacing: 8) {
                        Image(layer.state.symbol, size: 11)
                            .foregroundStyle(layer.state.color)
                            .frame(width: 16)
                        Text(verbatim: "#\(layer.number)")
                            .font(.ui(size: 12, weight: .medium))
                            .foregroundStyle(Color.themeMutedForeground)
                            .monospacedDigit()
                        Text(layer.title)
                            .font(.ui(size: 12.5))
                            .foregroundStyle(Color.themeForeground)
                            .lineLimit(1)
                        Spacer(minLength: 4)
                        if layer.current {
                            Text("This one")
                                .font(.ui(size: 11.5))
                                .foregroundStyle(Color.themeMutedStrongerForeground)
                        }
                    }
                    .padding(.horizontal, 12)
                    .frame(height: pressable(28))
                }
                .buttonStyle(.highlight(radius: Radius.sm, inset: EdgeInsets(top: 0, leading: 4, bottom: 0, trailing: 4)))
                .disabled(layer.current)
            }
        }
        .padding(.bottom, 6)
        .box(in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
    }
}

/// What stands between the pull request and merging, and the button its state calls for.
private struct MergeBox: View {
    @Environment(AppStore.self) private var store
    let page: PullRequestPage
    let working: PullRequestWork?
    let run: (PullRequestButton) -> Void
    @State private var showsAllChecks = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(page.statuses.enumerated()), id: \.element.id) { index, status in
                if index > 0 { PanelLine() }
                StatusRow(status: status, working: working, run: run)
                if status.kind == "checks" {
                    checks
                }
            }
            if page.primary != nil {
                PanelLine()
                actions
            }
        }
        .box(in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
    }

    /// The checks that need looking at and the running ones, and the rest when asked for.
    @ViewBuilder
    private var checks: some View {
        let settled = page.checks.filter { $0.tone == .success || $0.tone == .neutral }
        let shown = showsAllChecks ? page.checks : page.checks.filter { $0.tone != .success && $0.tone != .neutral }
        VStack(alignment: .leading, spacing: 0) {
            ForEach(shown) { check in
                CheckRow(check: check)
            }
            if !settled.isEmpty {
                ActionButton(
                    showsAllChecks ? "Hide Finished Checks" : "Show \(settled.count) Finished \(settled.count == 1 ? "Check" : "Checks")",
                    variant: .link, size: .small
                ) {
                    showsAllChecks.toggle()
                }
                .padding(.leading, 30)
            }
        }
        .padding(.bottom, 6)
    }

    private var actions: some View {
        HStack(spacing: 8) {
            if let primary = page.primary {
                HStack(spacing: 1) {
                    ActionButton(
                        primary.label, variant: ButtonVariant(style: primary.style), pending: working?.key == primary.key,
                        pendingTitle: primary.pendingLabel, joined: chooses(primary) ? .trailing : []
                    ) { run(primary) }
                    if chooses(primary) {
                        methodMenu(ButtonVariant(style: primary.style))
                    }
                }
                .disabled(working != nil)
            }
            Spacer(minLength: 0)
        }
        .padding(10)
    }

    /// The merge button lets the way it merges be chosen, when there are several.
    private func chooses(_ button: PullRequestButton) -> Bool {
        !page.methods.isEmpty && (button.action == "merge" || button.action == "enable_auto_merge")
    }

    private func methodMenu(_ variant: ButtonVariant) -> some View {
        ActionMenu(nil, help: "Choose how it merges", variant: variant, joined: .leading) {
            ForEach(page.methods) { choice in
                Toggle(choice.label, isOn: Binding { choice.method == page.method } set: { _ in
                    guard let method = choice.method, let target = store.panelTarget else { return }
                    store.sidePanel.chooseMethod(method, for: target, number: page.number)
                })
            }
        }
    }
}

private struct StatusRow: View {
    let status: PullRequestPage.Status
    let working: PullRequestWork?
    let run: (PullRequestButton) -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(symbol, size: 13)
                .foregroundStyle(status.tone.color)
                .frame(width: 18, height: scaled(17))
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(status.title)
                        .font(.ui(size: 13, weight: .medium))
                        .foregroundStyle(Color.themeForeground)
                    if let at = status.at {
                        Text(Time.ago(at))
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeMutedStrongerForeground)
                    }
                }
                if let detail = status.detail {
                    Text(detail)
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeMutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if !status.buttons.isEmpty {
                    HStack(spacing: 6) {
                        ForEach(status.buttons) { button in
                            ActionButton(button.label, variant: ButtonVariant(style: button.style), pending: working?.key == button.key) {
                                run(button)
                            }
                            .disabled(working != nil)
                        }
                    }
                    .padding(.top, 5)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
    }

    private var symbol: Symbol {
        switch (status.kind, status.tone) {
        case ("merged", _), ("auto_merge", _): .gitMerge
        case ("closed", _): .gitPullRequestClosed
        case ("draft", _): .gitPullRequestDraft
        case ("behind", _): .circleArrowDown
        case ("review", .warning): .eye
        case (_, .success): .circleCheck
        case ("conflicts", .danger): .triangleAlert
        case (_, .danger): .circleX
        case (_, .pending): .circleDashed
        default: .circleAlert
        }
    }
}

private struct CheckRow: View {
    @Environment(AppStore.self) private var store
    let check: PullRequestPage.Check

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(check.tone.color)
                .frame(width: 7, height: 7)
                .frame(width: 18)
            Text(check.name)
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeForeground)
                .lineLimit(1)
                .help(check.description ?? check.name)
            if let workflow = check.workflow {
                Text(workflow)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .lineLimit(1)
                    .layoutPriority(-1)
            }
            Spacer(minLength: 6)
            Text(check.label)
                .font(.ui(size: 12))
                .foregroundStyle(check.tone == .neutral ? Color.themeMutedStrongerForeground : check.tone.color)
            if let fix = check.fix {
                ActionButton("Fix", help: "Have the agent fix it", variant: .link, size: .small) { store.handOff(fix) }
            }
            if let url = check.url {
                ActionButton(icon: .squareArrowOutUpRight, help: "Show its details", size: .small) { Platform.open(url) }
            }
        }
        .padding(.leading, 12)
        .padding(.trailing, 6)
        .frame(minHeight: pressable(26))
    }
}
