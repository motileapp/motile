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
                    .task(id: PanelTrigger(target: target, path: String(number), version: store.workspaceVersion, asked: asked)) {
                        panel.loadPullRequest(of: target, number: number)
                    }
                    .task(id: panel.pullRequestReads) {
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
                .foregroundStyle(Color.themeText)
                .monospacedDigit()
                .padding(.leading, 2)
            if page.number == target.pullRequest, store.selectedThread?.watching == true {
                Image(.eye, size: 12)
                    .foregroundStyle(Color.themeLink)
                    .padding(.leading, 4)
                    .help("The agent hears when its checks finish, someone comments or it conflicts")
            }
        } else {
            Text("Pull Request")
                .font(.ui(size: 12.5, weight: .medium))
                .foregroundStyle(Color.themeText)
        }
        Spacer(minLength: 4)
        if let work = store.sidePanel.pullRequestWorking, work.key.hasPrefix("menu:") {
            Spinner(color: .themeSecondary)
                .frame(width: 11, height: 11)
            Text(work.label)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeSecondary)
                .lineLimit(1)
                .padding(.trailing, 4)
        }
        if let page, let url = page.url {
            IconOnlyButton(symbol: .squareArrowOutUpRight, help: "Open on GitHub") { Platform.open(url) }
        }
        if shown != nil, store.pullRequestsUnavailable == nil {
            IconOnlyButton(symbol: .rotateCw, help: "Read the pull request again") { asked += 1 }
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
                .foregroundStyle(Color.themeTertiary)
            Text("This branch has no pull request yet.")
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeSecondary)
            HStack(spacing: 8) {
                if let project = store.gitProject, let create = project.gitControl?.menu.first(where: { $0.action == "create_pr" }),
                    create.reason == nil
                {
                    PullRequestActionButton(label: "Create PR", style: "primary") { store.chooseGit(create, in: project) }
                }
                if store.pullRequestsExtended {
                    PullRequestActionButton(label: "Show All Pull Requests", style: "plain") {
                        store.sidePanel.open(.pullRequests)
                    }
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
            TextField("Link a PR by number or address", text: $text)
                .textFieldStyle(.plain)
                .font(.ui(size: 12.5))
                .padding(.horizontal, 9)
                .frame(height: scaled(28))
                .background(Color.themeField, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
                }
                .onSubmit(submit)
            PullRequestActionButton(label: "Link", style: "plain", action: submit)
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
    /// The key of the action that runs, and what it says.
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
                activityTitle
                ForEach(page.activity) { entry in
                    ActivityRow(entry: entry, actions: actions)
                }
                if page.state != .merged {
                    commentBox
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
                panel.edit(["kind": "body", "body": body], key: "menu:body", label: "Saving the description…", on: target, number: number)
            }
        }
    }

    private var actions: PullRequestActions {
        PullRequestActions(
            react: { subject, kind, on in
                panel.edit(["kind": "react", "subject": subject, "reaction": kind, "on": on], on: target, number: number)
            },
            reply: { thread, body, done in
                panel.edit(["kind": "reply", "thread": thread, "body": body], key: "reply:\(thread)", label: "Replying…", on: target, number: number, done: done)
            },
            resolve: { thread, resolved in
                let label = resolved ? "Resolving…" : "Opening…"
                panel.edit(["kind": "resolve", "thread": thread, "resolved": resolved], key: "resolve:\(thread)", label: label, on: target, number: number)
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
                        .foregroundStyle(Color.themeText)
                        .textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                    moreMenu
                        .padding(.top, -4)
                }
            }
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                StateLabel(state: page.state)
                Text(page.byline)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 6) {
                BranchName(name: page.base)
                Image(.arrowLeft, size: 10)
                    .foregroundStyle(Color.themeTertiary)
                BranchName(name: page.head)
                Spacer(minLength: 8)
                Button {
                    panel.showDiff(.pullRequest(page.number))
                } label: {
                    HStack(spacing: 6) {
                        Image(.diff, size: 11)
                            .foregroundStyle(Color.themeSecondary)
                        Text(page.files == 1 ? "1 file" : "\(page.files) files")
                            .foregroundStyle(Color.themeText)
                        if page.additions + page.deletions > 0 {
                            Text(AttributedString(LineCountText.text(added: page.additions, removed: page.deletions)))
                        }
                    }
                    .font(.ui(size: 12))
                    .padding(.horizontal, 6)
                    .frame(height: pressable(24))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.highlight(radius: 6))
                .help("Show what it changes")
            }
            if extended, let below = page.stackedOn {
                Button {
                    panel.showPullRequest(below.number, of: target)
                } label: {
                    HStack(spacing: 6) {
                        Image(.layers, size: 11)
                            .foregroundStyle(Color.themeSecondary)
                        (Text("Stacked on ").foregroundStyle(Color.themeSecondary)
                            + Text(verbatim: "#\(below.number) ").foregroundStyle(below.state.color)
                            + Text(below.title).foregroundStyle(Color.themeText))
                            .lineLimit(1)
                    }
                    .font(.ui(size: 12))
                    .padding(.horizontal, 6)
                    .frame(height: pressable(24))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.highlight(radius: 6))
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
            TextField("Title", text: binding)
                .textFieldStyle(.plain)
                .font(.ui(size: 15, weight: .semibold))
                .padding(.horizontal, 8)
                .frame(height: scaled(32))
                .background(Color.themeField, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 7, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
                }
                .focused($titleFocused)
                .onSubmit(saveTitle)
                .onAppear { titleFocused = true }
                #if os(macOS)
                .onExitCommand { title = nil }
                #endif
            HStack(spacing: 8) {
                Spacer()
                PullRequestActionButton(label: "Cancel", style: "plain") { title = nil }
                    .disabled(working)
                PullRequestActionButton(label: "Save", style: "primary", working: working ? "Saving…" : nil, action: saveTitle)
                    .disabled(working || editing.trimmingCharacters(in: .whitespaces).isEmpty || editing == page.title)
            }
        }
    }

    private func saveTitle() {
        guard let edited = title?.trimmingCharacters(in: .whitespaces), !edited.isEmpty, edited != page.title else { return }
        panel.edit(["kind": "title", "title": edited], key: "title", label: "Saving…", on: target, number: number) { title = nil }
    }

    /// Who reviews it and its labels, each with a menu to change them when the user may.
    @ViewBuilder
    private var people: some View {
        let reviews = !page.reviewers.isEmpty || !page.reviewerChoices.isEmpty
        let labelled = !page.labels.isEmpty || !page.labelChoices.isEmpty
        if reviews || labelled {
            VStack(alignment: .leading, spacing: 6) {
                if reviews {
                    PeopleRow(title: "Reviewers", empty: page.reviewers.isEmpty ? "None yet" : nil, choices: page.reviewerChoices, help: "Ask for a review") {
                        ForEach(page.reviewers) { ReviewerChip(reviewer: $0) }
                    } toggle: { name, on in
                        let edit: JSON = ["kind": "reviewers", "add": on ? [name] : [], "remove": on ? [] : [name]]
                        panel.edit(edit, key: "menu:reviewers", label: "Updating reviewers…", on: target, number: number)
                    }
                }
                if labelled {
                    PeopleRow(title: "Labels", empty: page.labels.isEmpty ? "None yet" : nil, choices: page.labelChoices, help: "Change the labels") {
                        ForEach(page.labels, id: \.name) { LabelChip(name: $0.name, color: $0.color) }
                    } toggle: { name, on in
                        let edit: JSON = ["kind": "labels", "add": on ? [name] : [], "remove": on ? [] : [name]]
                        panel.edit(edit, key: "menu:labels", label: "Updating labels…", on: target, number: number)
                    }
                }
            }
            .padding(.top, 2)
        }
    }

    /// Everything else there is to do, after what the merge box offers.
    private var moreMenu: some View {
        let working = panel.pullRequestWorking != nil
        let thread = store.selectedThread
        let ownPullRequest = thread?.pullRequest?.number == number
        return Menu {
            ForEach(page.menu) { button in
                Button(role: button.style == "danger" ? .destructive : nil) { run(button, fromMenu: true) } label: { Text(button.label) }
                    .disabled(working && button.prompt == nil)
            }
            if extended {
                Divider()
                if page.canEdit {
                    Button("Edit Title") { title = page.title }
                    Button("Edit Description…") { describing = true }
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
        } label: {
            Image(.ellipsis, size: 14)
                .foregroundStyle(Color.themeSecondary)
                .frame(width: scaled(28), height: scaled(28))
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight()
        .help("More")
    }

    /// Parts the merge box, which is where the pull request stands now, from what happened on it.
    private var activityTitle: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text("Activity")
                .font(.ui(size: 13, weight: .semibold))
                .foregroundStyle(Color.themeText)
            Text("Oldest first")
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeTertiary)
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
            WritingField(text: $comment, placeholder: pending.isEmpty ? "Leave a comment" : "Say something with your review (optional)")
            HStack(spacing: 8) {
                if let close = page.withComment, pending.isEmpty {
                    PullRequestActionButton(label: close.label, style: "plain", working: work?.key == "close" ? "\(close.action == "close" ? "Closing" : "Reopening")…" : nil) {
                        choose(close, key: "close")
                    }
                    .disabled(busy || written.isEmpty)
                }
                Spacer(minLength: 0)
                ForEach(page.verdicts) { verdict in
                    let key = verdict.action
                    PullRequestActionButton(label: verdict.label, style: "plain", working: work?.key == key ? (key == "approve" ? "Approving…" : "Sending…") : nil) {
                        if pending.isEmpty {
                            choose(verdict, key: key)
                        } else {
                            panel.review(key, body: written, key: key, label: "Sending…", on: target, number: number) { comment = "" }
                        }
                    }
                    .disabled(busy || (verdict.action == "request_changes" && written.isEmpty && pending.isEmpty))
                }
                PullRequestActionButton(
                    label: pending.isEmpty ? "Comment" : "Send Review", style: "primary",
                    working: work?.key == "comment" ? "Sending…" : nil
                ) {
                    if pending.isEmpty {
                        panel.act("comment", text: written, key: "comment", label: "Sending…", on: target, number: number) { comment = "" }
                    } else {
                        panel.review("comment", body: written, key: "comment", label: "Sending…", on: target, number: number) { comment = "" }
                    }
                }
                .disabled(busy || (written.isEmpty && pending.isEmpty))
            }
        }
        .padding(.top, 4)
    }

    private func choose(_ choice: PullRequestChoice, key: String) {
        let text = comment.trimmingCharacters(in: .whitespacesAndNewlines)
        panel.act(choice.action, method: choice.method, text: text, key: key, label: choice.label, on: target, number: number) {
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
        panel.act(action, method: button.method, key: button.key, label: Self.working(button), on: target, number: number)
    }

    /// What a button says while its action runs.
    static func working(_ button: PullRequestButton) -> String {
        switch button.action {
        case "merge": "Merging…"
        case "enable_auto_merge": "Turning on auto-merge…"
        case "disable_auto_merge": "Cancelling auto-merge…"
        case "update_branch": "Updating…"
        case "ready": "Marking ready…"
        case "draft": "Converting to draft…"
        case "revert": "Opening a revert…"
        case "close": "Closing…"
        case "reopen": "Reopening…"
        default: "\(button.label)…"
        }
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
    @ViewBuilder let chips: Chips
    let toggle: (String, Bool) -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            Text(title)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeTertiary)
                .frame(width: 70, alignment: .leading)
            if let empty {
                Text(empty)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeTertiary)
            } else {
                FlowRow(spacing: 5) { chips }
            }
            if !choices.isEmpty {
                Menu {
                    ForEach(choices) { choice in
                        Toggle(choice.name, isOn: Binding { choice.on } set: { toggle(choice.name, $0) })
                    }
                } label: {
                    Image(.plus, size: 11)
                        .foregroundStyle(Color.themeSecondary)
                        .frame(width: scaled(22), height: scaled(22))
                        .contentShape(Rectangle())
                }
                .menuStyle(.button)
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .fixedSize()
                .hoverHighlight(radius: 6)
                .help(help)
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
                .foregroundStyle(Color.themeSecondary)
                .padding(.horizontal, 12)
                .padding(.top, 9)
                .padding(.bottom, 4)
            ForEach(comments) { comment in
                HStack(alignment: .top, spacing: 8) {
                    Text(verbatim: "\(URL(fileURLWithPath: comment.path).lastPathComponent):\(comment.line)")
                        .font(.ui(size: 11.5, design: .monospaced))
                        .foregroundStyle(Color.themeLink)
                    Text(comment.body)
                        .font(.ui(size: 12.5))
                        .foregroundStyle(Color.themeText)
                        .lineLimit(2)
                    Spacer(minLength: 4)
                    IconOnlyButton(symbol: .x, help: "Take it back", size: scaled(20), symbolSize: 10, faded: true) { remove(comment) }
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 4)
            }
        }
        .padding(.bottom, 6)
        .background(Color.themeBubble, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
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
                    .foregroundStyle(Color.themeSecondary)
                Text("Stack")
                    .font(.ui(size: 13, weight: .medium))
                    .foregroundStyle(Color.themeText)
                Text("\(stack.layers.count) pull requests onto \(stack.base)")
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeTertiary)
                Spacer(minLength: 4)
                if let url = stack.url {
                    LinkButton("Open on GitHub") { Platform.open(url) }
                        .help("A stack merges on GitHub, bottom first")
                }
            }
            .padding(.horizontal, 12)
            .padding(.top, 8)
            .padding(.bottom, 4)
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
                            .foregroundStyle(Color.themeSecondary)
                            .monospacedDigit()
                        Text(layer.title)
                            .font(.ui(size: 12.5, weight: layer.current ? .semibold : .regular))
                            .foregroundStyle(Color.themeText)
                            .lineLimit(1)
                        Spacer(minLength: 4)
                        if layer.current {
                            Text("This one")
                                .font(.ui(size: 11.5))
                                .foregroundStyle(Color.themeTertiary)
                        }
                    }
                    .padding(.horizontal, 12)
                    .frame(height: pressable(28))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.highlight(radius: 6, inset: EdgeInsets(top: 0, leading: 4, bottom: 0, trailing: 4)))
                .disabled(layer.current)
            }
        }
        .padding(.bottom, 6)
        .background(Color.themeBubble, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
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
        .background(Color.themeBubble, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
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
                Button {
                    showsAllChecks.toggle()
                } label: {
                    Text(showsAllChecks ? "Hide Finished Checks" : "Show \(settled.count) Finished \(settled.count == 1 ? "Check" : "Checks")")
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeLink)
                        .padding(.horizontal, 8)
                        .frame(height: pressable(24))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.highlight(radius: 6))
                .padding(.leading, 30)
            }
        }
        .padding(.bottom, 6)
    }

    private var actions: some View {
        HStack(spacing: 8) {
            if let primary = page.primary {
                HStack(spacing: 1) {
                    PullRequestActionButton(
                        label: primary.label, style: primary.style, joined: chooses(primary),
                        working: working?.key == primary.key ? working?.label : nil
                    ) { run(primary) }
                    if chooses(primary) {
                        methodMenu
                            .opacity(working != nil ? 0.6 : 1)
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

    private var methodMenu: some View {
        Menu {
            ForEach(page.methods) { choice in
                Toggle(choice.label, isOn: Binding { choice.method == page.method } set: { _ in
                    guard let method = choice.method, let target = store.panelTarget else { return }
                    store.sidePanel.chooseMethod(method, for: target, number: page.number)
                })
            }
        } label: {
            Image(.chevronDown, size: 9)
                .foregroundStyle(Color.white)
                .frame(width: scaled(24), height: scaled(28))
                .background(Color.themePrimary, in: UnevenRoundedRectangle(bottomTrailingRadius: 7, topTrailingRadius: 7, style: .continuous))
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(DimButtonStyle())
        .menuIndicator(.hidden)
        .fixedSize()
        .help("Choose how it merges")
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
                        .foregroundStyle(Color.themeText)
                    if let at = status.at {
                        Text(Time.ago(at))
                            .font(.ui(size: 12))
                            .foregroundStyle(Color.themeTertiary)
                    }
                }
                if let detail = status.detail {
                    Text(detail)
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if !status.buttons.isEmpty {
                    HStack(spacing: 6) {
                        ForEach(status.buttons) { button in
                            PullRequestActionButton(
                                label: button.label, style: button.style,
                                working: working?.key == button.key ? working?.label : nil
                            ) { run(button) }
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
                .foregroundStyle(Color.themeText)
                .lineLimit(1)
                .help(check.description ?? check.name)
            if let workflow = check.workflow {
                Text(workflow)
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeTertiary)
                    .lineLimit(1)
                    .layoutPriority(-1)
            }
            Spacer(minLength: 6)
            Text(check.label)
                .font(.ui(size: 12))
                .foregroundStyle(check.tone == .neutral ? Color.themeTertiary : check.tone.color)
            if let fix = check.fix {
                LinkButton("Fix") { store.handOff(fix) }
                    .help("Have the agent fix it")
            }
            if let url = check.url {
                IconOnlyButton(symbol: .squareArrowOutUpRight, help: "Show its details", size: scaled(22), symbolSize: 11, faded: true) {
                    Platform.open(url)
                }
            }
        }
        .padding(.leading, 12)
        .padding(.trailing, 8)
        .frame(minHeight: pressable(26))
    }
}
