import SwiftUI

/// One thing that happened on the pull request: its opening with the description, commits,
/// comments, reviews and conversations on lines, and how it ended.
struct ActivityRow: View {
    let entry: PullRequestPage.Entry
    let actions: PullRequestActions

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(symbol, size: 12)
                .foregroundStyle(entry.tone == .neutral ? Color.themeSecondary : entry.tone.color)
                .frame(width: 18, height: scaled(17))
            VStack(alignment: .leading, spacing: 6) {
                byline
                if !entry.commits.isEmpty {
                    commits
                }
                if let thread = entry.thread {
                    ThreadCard(thread: thread, actions: actions)
                } else if !entry.body.isEmpty {
                    PullRequestTextView(blocks: entry.body)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.themeBubble, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                }
                if let subject = entry.subject, entry.thread == nil, !entry.reactions.isEmpty {
                    ReactionBar(reactions: entry.reactions) { kind, on in actions.react(subject, kind, on) }
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
            if let subject = entry.subject, entry.thread == nil, entry.reactions.isEmpty {
                ReactionPicker(reactions: []) { kind, on in actions.react(subject, kind, on) }
            }
            if entry.kind == "opened", let edit = actions.editDescription {
                IconOnlyButton(symbol: .pencil, help: "Edit the description", size: scaled(20), symbolSize: 10, faded: true, action: edit)
            }
            if let url = entry.url {
                IconOnlyButton(symbol: .squareArrowOutUpRight, help: "Open on GitHub", size: scaled(20), symbolSize: 10, faded: true) {
                    Platform.open(url)
                }
            }
        }
    }

    /// The commits, each one opening what it changed.
    private var commits: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(entry.commits) { commit in
                Button {
                    actions.showCommit(commit.sha)
                } label: {
                    HStack(spacing: 8) {
                        Text(commit.oid)
                            .font(.ui(size: 11.5, design: .monospaced))
                            .foregroundStyle(Color.themeLink)
                        Text(commit.headline)
                            .font(.ui(size: 12.5))
                            .foregroundStyle(Color.themeText)
                            .lineLimit(1)
                            .truncationMode(.tail)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 6)
                    .frame(height: pressable(24))
                }
                .buttonStyle(.highlight(radius: 6))
                .disabled(commit.sha.isEmpty)
                .help("Show what this commit changed")
            }
        }
        .padding(.leading, -6)
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
        case "thread": .code
        default: .messageSquareText
        }
    }
}

/// A conversation on a line: the line it is on, what was said, and a reply. A resolved one is
/// folded until it is opened.
struct ThreadCard: View {
    let thread: PullRequestPage.Thread
    let actions: PullRequestActions
    @State private var open: Bool?
    @State private var replying = false
    @State private var reply = ""

    private var isOpen: Bool { open ?? !thread.resolved }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            if isOpen {
                if !thread.hunk.isEmpty {
                    HunkView(lines: thread.hunk)
                }
                ForEach(thread.comments) { comment in
                    PanelLine()
                    CommentView(comment: comment, react: actions.react)
                }
                PanelLine()
                footer
            }
        }
        .background(Color.themeBubble, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private var header: some View {
        Button {
            open = !isOpen
        } label: {
            HStack(spacing: 6) {
                Image(isOpen ? .chevronDown : .chevronRight, size: 8)
                    .foregroundStyle(Color.themeTertiary)
                    .frame(width: 10)
                Image(FileSymbol.symbol(for: thread.path), size: 11)
                    .foregroundStyle(Color.themeSecondary)
                Text(thread.path)
                    .font(.ui(size: 12, weight: .medium))
                    .foregroundStyle(Color.themeText)
                    .lineLimit(1)
                    .truncationMode(.head)
                if let line = thread.line {
                    Text("line \(line)")
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeTertiary)
                        .fixedSize()
                }
                if thread.outdated { tag("Outdated", .themeWarning) }
                if thread.resolved { tag("Resolved", .themeSuccess) }
                Spacer(minLength: 0)
                if !isOpen {
                    Text(thread.comments.count == 1 ? "1 comment" : "\(thread.comments.count) comments")
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeTertiary)
                }
            }
            .padding(.horizontal, 12)
            .frame(height: pressable(32))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func tag(_ text: String, _ color: Color) -> some View {
        Text(text)
            .font(.ui(size: 10.5, weight: .semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 6)
            .frame(height: scaled(17))
            .background(color.opacity(0.14), in: Capsule())
            .fixedSize()
    }

    @ViewBuilder
    private var footer: some View {
        let busy = actions.working != nil
        if replying {
            VStack(alignment: .leading, spacing: 8) {
                WritingField(text: $reply, placeholder: "Reply", height: 64)
                HStack(spacing: 8) {
                    Spacer()
                    PullRequestActionButton(label: "Cancel", style: "plain") {
                        replying = false
                        reply = ""
                    }
                    PullRequestActionButton(label: "Reply", style: "primary", working: actions.working?.key == "reply:\(thread.id)" ? "Replying…" : nil) {
                        actions.reply(thread.id, reply.trimmingCharacters(in: .whitespacesAndNewlines)) {
                            replying = false
                            reply = ""
                        }
                    }
                    .disabled(busy || reply.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .padding(10)
        } else {
            HStack(spacing: 6) {
                PullRequestActionButton(label: "Reply", style: "plain") { replying = true }
                if let fix = thread.fix {
                    PullRequestActionButton(label: "Fix", style: "plain") { actions.handOff(fix) }
                        .help("Have the agent do what it asks")
                }
                Spacer(minLength: 0)
                if thread.canResolve {
                    let resolving = actions.working?.key == "resolve:\(thread.id)"
                    PullRequestActionButton(
                        label: thread.resolved ? "Unresolve" : "Resolve", style: "plain",
                        working: resolving ? actions.working?.label : nil
                    ) {
                        actions.resolve(thread.id, !thread.resolved)
                    }
                    .disabled(busy)
                }
            }
            .padding(10)
        }
    }
}

/// The lines of the diff a conversation was written under, the line itself last.
private struct HunkView: View {
    let lines: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(lines.enumerated()), id: \.offset) { index, line in
                Text(line.isEmpty ? " " : line)
                    .font(.ui(size: 11.5, design: .monospaced))
                    .foregroundStyle(Color.themeText)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .padding(.horizontal, 12)
                    .frame(maxWidth: .infinity, minHeight: 18, alignment: .leading)
                    .background(fill(line).opacity(index == lines.count - 1 ? 1 : 0.7))
            }
        }
        .padding(.bottom, 6)
    }

    private func fill(_ line: String) -> Color {
        if line.hasPrefix("+") { return .themeSuccess.opacity(0.14) }
        if line.hasPrefix("-") { return .themeDanger.opacity(0.14) }
        return .clear
    }
}

/// A comment in a conversation on a line.
private struct CommentView: View {
    let comment: PullRequestPage.Comment
    let react: (_ subject: String, _ kind: String, _ on: Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 0) {
                (Text(comment.author).fontWeight(.semibold).foregroundStyle(Color.themeText)
                    + Text(" · \(Time.ago(comment.at))").foregroundStyle(Color.themeTertiary))
                    .font(.ui(size: 12.5))
                Spacer(minLength: 4)
                if comment.reactions.isEmpty {
                    ReactionPicker(reactions: []) { kind, on in react(comment.id, kind, on) }
                }
                if let url = comment.url {
                    IconOnlyButton(symbol: .squareArrowOutUpRight, help: "Open on GitHub", size: scaled(20), symbolSize: 10, faded: true) {
                        Platform.open(url)
                    }
                }
            }
            PullRequestTextView(blocks: comment.body)
            if !comment.reactions.isEmpty {
                ReactionBar(reactions: comment.reactions) { kind, on in react(comment.id, kind, on) }
            }
        }
        .padding(12)
    }
}
