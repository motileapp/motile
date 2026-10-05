import SwiftUI

/// The pull request of the branch the thread works on: where it stands, what holds it up, the
/// button its state calls for, and what was said on it, with a box to say more.
struct PullRequestSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    @State private var asked = 0

    var body: some View {
        let panel = store.sidePanel
        VStack(spacing: 0) {
            PanelBar { bar(panel.pullRequest.value) }
            if let reason = store.pullRequestUnavailable {
                unavailable(reason)
            } else if let number = target.pullRequest {
                content(number)
                    .task(id: PanelTrigger(target: target, version: store.workspaceVersion, asked: asked)) {
                        panel.loadPullRequest(of: target, number: number)
                    }
                    .task(id: panel.pullRequestReads) {
                        // While GitHub is still working something out, it is asked again.
                        guard panel.pullRequest.value?.settling == true else { return }
                        try? await Task.sleep(for: .seconds(10))
                        guard !Task.isCancelled else { return }
                        panel.loadPullRequest(of: target, number: number)
                    }
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
        } else {
            Text("Pull Request")
                .font(.ui(size: 12.5, weight: .medium))
                .foregroundStyle(Color.themeText)
        }
        Spacer(minLength: 4)
        if let working = store.sidePanel.pullRequestWorking {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.7)
                .frame(width: 16, height: 16)
            Text(working)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeSecondary)
                .lineLimit(1)
                .padding(.trailing, 4)
        }
        if let page, let url = page.url {
            IconOnlyButton(symbol: .squareArrowOutUpRight, help: "Open on GitHub") { Platform.open(url) }
        }
        if store.pullRequestUnavailable == nil {
            IconOnlyButton(symbol: .rotateCw, help: "Read the pull request again") { asked += 1 }
        }
    }

    /// Why there is nothing to show, and the way to a pull request when the branch can have one.
    private func unavailable(_ reason: String) -> some View {
        VStack(spacing: 12) {
            Text(reason)
                .font(.ui(size: 12.5))
                .foregroundStyle(Color.themeSecondary)
                .multilineTextAlignment(.center)
            if let project = store.gitProject, let create = project.gitControl?.menu.first(where: { $0.action == "create_pr" }),
                create.reason == nil, target.pullRequest == nil
            {
                PullRequestActionButton(label: "Create PR", style: "primary") { store.chooseGit(create, in: project) }
            }
        }
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder
    private func content(_ number: Int) -> some View {
        let panel = store.sidePanel
        if let notice = panel.pullRequestNotice {
            PullRequestNoticeBar(notice: notice)
        }
        switch panel.pullRequest {
        case .loading:
            PanelLoading()
        case .failed(let message):
            PanelMessage(text: message, failed: true)
        case .ready(let page):
            PullRequestPageView(page: page, target: target, number: number)
        }
    }
}

private struct PullRequestPageView: View {
    @Environment(AppStore.self) private var store
    let page: PullRequestPage
    let target: PanelTarget
    let number: Int
    @State private var confirming: PullRequestButton?
    @State private var comment = ""

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                MergeBox(page: page, run: run)
                ForEach(page.activity) { entry in
                    ActivityRow(entry: entry)
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
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .top, spacing: 8) {
                Text(page.title)
                    .font(.ui(size: 15, weight: .semibold))
                    .foregroundStyle(Color.themeText)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
                moreMenu
                    .padding(.top, -4)
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
                    store.sidePanel.showDiff(.pullRequest(page.number))
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
        }
    }

    /// Everything else there is to do, after what the merge box offers.
    private var moreMenu: some View {
        let working = store.sidePanel.pullRequestWorking != nil
        return Menu {
            ForEach(page.menu) { button in
                Button(role: button.style == "danger" ? .destructive : nil) { run(button) } label: { Text(button.label) }
                    .disabled(working && button.prompt == nil)
            }
            if !page.menu.isEmpty { Divider() }
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

    private var commentBox: some View {
        let working = store.sidePanel.pullRequestWorking != nil
        let written = comment.trimmingCharacters(in: .whitespacesAndNewlines)
        return VStack(alignment: .leading, spacing: 8) {
            CommentField(text: $comment)
            HStack(spacing: 8) {
                if let close = page.withComment {
                    PullRequestActionButton(label: close.label, style: "plain") { choose(close) }
                        .disabled(working || written.isEmpty)
                }
                Spacer(minLength: 0)
                ForEach(page.verdicts) { verdict in
                    PullRequestActionButton(label: verdict.label, style: "plain") { choose(verdict) }
                        .disabled(working || (verdict.action == "request_changes" && written.isEmpty))
                }
                PullRequestActionButton(label: "Comment", style: "primary") {
                    store.sidePanel.act("comment", text: written, label: "Commenting", on: target, number: number) { comment = "" }
                }
                .disabled(working || written.isEmpty)
            }
        }
        .padding(.top, 4)
    }

    private func choose(_ choice: PullRequestChoice) {
        let text = comment.trimmingCharacters(in: .whitespacesAndNewlines)
        store.sidePanel.act(choice.action, method: choice.method, text: text, label: choice.label, on: target, number: number) {
            comment = ""
        }
    }

    /// A button's prompt goes to the composer; its action runs, after asking when it says to.
    private func run(_ button: PullRequestButton) {
        if let prompt = button.prompt {
            store.handOff(prompt)
            return
        }
        guard button.confirm == nil else {
            confirming = button
            return
        }
        perform(button)
    }

    private func perform(_ button: PullRequestButton) {
        guard let action = button.action else { return }
        store.sidePanel.act(action, method: button.method, label: Self.working(button), on: target, number: number)
    }

    /// What the bar says while the button's action runs.
    private static func working(_ button: PullRequestButton) -> String {
        switch button.action {
        case "merge": "Merging"
        case "enable_auto_merge": "Turning on auto-merge"
        case "update_branch": "Updating the branch"
        case "revert": "Opening a revert"
        case "close": "Closing"
        case "reopen": "Reopening"
        default: button.label
        }
    }

    private var confirmingShown: Binding<Bool> {
        Binding { confirming != nil } set: { shown in
            if !shown { confirming = nil }
        }
    }
}

/// What stands between the pull request and merging, and the button its state calls for.
private struct MergeBox: View {
    @Environment(AppStore.self) private var store
    let page: PullRequestPage
    let run: (PullRequestButton) -> Void
    @State private var showsAllChecks = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(page.statuses.enumerated()), id: \.element.id) { index, status in
                if index > 0 { PanelLine() }
                StatusRow(status: status, run: run)
                if status.kind == "checks" {
                    checks
                }
            }
            if page.primary != nil {
                PanelLine()
                actions
            }
        }
        .background(Color.themeRaised, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.themeBorder, lineWidth: 1)
        }
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
        let working = store.sidePanel.pullRequestWorking != nil
        return HStack(spacing: 8) {
            if let primary = page.primary {
                HStack(spacing: 1) {
                    PullRequestActionButton(label: primary.label, style: primary.style, joined: chooses(primary)) { run(primary) }
                    if chooses(primary) {
                        methodMenu
                    }
                }
                .disabled(working)
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
                            PullRequestActionButton(label: button.label, style: button.style) { run(button) }
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

/// One thing that happened on the pull request: its opening with the description, commits,
/// comments and reviews, and how it ended.
private struct ActivityRow: View {
    let entry: PullRequestPage.Entry

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(symbol, size: 12)
                .foregroundStyle(entry.tone == .neutral ? Color.themeSecondary : entry.tone.color)
                .frame(width: 18, height: scaled(17))
            VStack(alignment: .leading, spacing: 6) {
                byline
                ForEach(Array(entry.commits.enumerated()), id: \.offset) { _, commit in
                    HStack(spacing: 8) {
                        Text(commit.oid)
                            .font(.ui(size: 11.5, design: .monospaced))
                            .foregroundStyle(Color.themeTertiary)
                        Text(commit.headline)
                            .font(.ui(size: 12.5))
                            .foregroundStyle(Color.themeText)
                            .lineLimit(1)
                            .truncationMode(.tail)
                    }
                }
                if !entry.body.isEmpty {
                    PullRequestTextView(blocks: entry.body)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.themeRaised, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                        .overlay {
                            RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.themeBorder, lineWidth: 1)
                        }
                }
            }
        }
    }

    private var byline: some View {
        HStack(alignment: .firstTextBaseline, spacing: 0) {
            (Text(entry.author).fontWeight(.semibold).foregroundStyle(Color.themeText)
                + Text(entry.author.isEmpty ? entry.said : " \(entry.said)").foregroundStyle(Color.themeSecondary)
                + Text(" · \(Time.ago(entry.at))").foregroundStyle(Color.themeTertiary))
                .font(.ui(size: 12.5))
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 6)
            if let url = entry.url {
                IconOnlyButton(symbol: .squareArrowOutUpRight, help: "Open on GitHub", size: scaled(20), symbolSize: 10, faded: true) {
                    Platform.open(url)
                }
            }
        }
    }

    private var symbol: Symbol {
        switch entry.kind {
        case "opened": .gitPullRequest
        case "commits": .gitCommitHorizontal
        case "merged": .gitMerge
        case "closed": .gitPullRequestClosed
        case "review" where entry.tone == .success: .circleCheck
        case "review" where entry.tone == .danger: .circleAlert
        case "review": .eye
        default: .messageSquareText
        }
    }
}

/// Markdown from the pull request, drawn as the transcript draws a reply.
struct PullRequestTextView: View {
    let blocks: [PullRequestText]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                switch block {
                case .prose(let text):
                    ProseText(text: text)
                case .code(let code):
                    ProseText(text: code)
                        .padding(10)
                        .background(Color.themeField, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
            }
        }
    }
}

/// The pull request's state, as a label in its colour.
private struct StateLabel: View {
    let state: PullRequest.State

    var body: some View {
        HStack(spacing: 4) {
            Image(state.symbol, size: 11)
            Text(state.title)
                .font(.ui(size: 11.5, weight: .semibold))
        }
        .foregroundStyle(state.color)
        .padding(.horizontal, 7)
        .frame(height: scaled(20))
        .background(state.color.opacity(0.14), in: Capsule())
    }
}

private struct BranchName: View {
    let name: String

    var body: some View {
        Text(name)
            .font(.ui(size: 11.5, design: .monospaced))
            .foregroundStyle(Color.themeText)
            .lineLimit(1)
            .truncationMode(.middle)
            .padding(.horizontal, 6)
            .frame(height: scaled(20))
            .background(Color.themeHover, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
            .textSelection(.enabled)
    }
}

/// A button of the pull request's tab, in the panel's colours: filled for what the state calls
/// for, red for what fixes it, quiet for the rest.
struct PullRequestActionButton: View {
    @Environment(\.isEnabled) private var isEnabled
    let label: String
    let style: String
    /// The method menu sits right of it, so its right corners are square.
    var joined = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.ui(size: 12, weight: .medium))
                .foregroundStyle(foreground)
                .lineLimit(1)
                .padding(.horizontal, 11)
                .frame(height: scaled(28))
                .background(background, in: shape)
                .overlay {
                    if style == "danger" { shape.strokeBorder(Color.themeDanger.opacity(0.6), lineWidth: 1) }
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(DimButtonStyle())
        .opacity(isEnabled ? 1 : 0.45)
        .fixedSize()
    }

    private var shape: UnevenRoundedRectangle {
        UnevenRoundedRectangle(
            topLeadingRadius: 7, bottomLeadingRadius: 7, bottomTrailingRadius: joined ? 0 : 7, topTrailingRadius: joined ? 0 : 7,
            style: .continuous)
    }

    private var foreground: Color {
        switch style {
        case "primary": .white
        case "danger": .themeDanger
        default: .themeText
        }
    }

    private var background: Color {
        switch style {
        case "primary": .themePrimary
        case "danger": .themeDanger.opacity(0.1)
        default: .themeSelected
        }
    }
}

/// What the last action did or why it couldn't, under the tab's bar.
private struct PullRequestNoticeBar: View {
    @Environment(AppStore.self) private var store
    let notice: PullRequestNotice

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .top, spacing: 8) {
                Image(notice.failed ? .circleAlert : .circleCheck, size: 13)
                    .foregroundStyle(notice.failed ? Color.themeDanger : Color.themeSuccess)
                    .frame(height: scaled(17))
                Text(notice.text)
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeText)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 4)
                if let url = notice.url {
                    LinkButton("Open") { Platform.open(url) }
                }
                IconOnlyButton(symbol: .x, help: "Close", size: scaled(20), symbolSize: 10, faded: true) {
                    store.sidePanel.dismissPullRequestNotice()
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(notice.failed ? Color.themeDanger.opacity(0.08) : Color.themeSuccess.opacity(0.08))
            PanelLine()
        }
    }
}

private struct CommentField: View {
    @Binding var text: String

    var body: some View {
        TextEditor(text: $text)
            .font(.ui(size: 12.5))
            .scrollContentBackground(.hidden)
            .padding(.horizontal, 4)
            .padding(.vertical, 6)
            .frame(height: 84)
            .background(Color.themeField, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 8, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
            }
            .overlay(alignment: .topLeading) {
                if text.isEmpty {
                    Text("Leave a comment")
                        .font(.ui(size: 12.5))
                        .foregroundStyle(Color.themeTertiary)
                        .padding(.horizontal, 9)
                        .padding(.vertical, 6)
                        .allowsHitTesting(false)
                }
            }
    }
}

extension PullRequest.State {
    var title: String {
        switch self {
        case .open: "Open"
        case .draft: "Draft"
        case .merged: "Merged"
        case .closed: "Closed"
        }
    }
}

extension PullRequestPage.Tone {
    var color: Color {
        switch self {
        case .success: .themeSuccess
        case .danger: .themeDanger
        case .warning: .themeWarning
        case .pending: .themeWorking
        case .neutral: .themeSecondary
        case .merged: .themeMerged
        }
    }
}

/// Text set by `Typesetter`, drawn in a text view of the transcript's, as tall as it needs.
struct ProseText {
    let text: NSAttributedString

    fileprivate static func height(of view: RowTextView, proposal: ProposedViewSize) -> CGSize? {
        guard let width = proposal.width, width.isFinite, width > 0 else { return nil }
        return CGSize(width: width, height: view.height(forWidth: width))
    }
}

#if os(macOS)
extension ProseText: NSViewRepresentable {
    func makeNSView(context: Context) -> RowTextView {
        RowTextView.make()
    }

    func updateNSView(_ view: RowTextView, context: Context) {
        view.content = text
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView view: RowTextView, context: Context) -> CGSize? {
        Self.height(of: view, proposal: proposal)
    }
}
#else
extension ProseText: UIViewRepresentable {
    func makeUIView(context: Context) -> RowTextView {
        RowTextView.make()
    }

    func updateUIView(_ view: RowTextView, context: Context) {
        view.content = text
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView view: RowTextView, context: Context) -> CGSize? {
        Self.height(of: view, proposal: proposal)
    }
}
#endif
