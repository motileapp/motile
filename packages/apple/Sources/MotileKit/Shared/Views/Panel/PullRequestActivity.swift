import SwiftUI

/// One thing that happened on the pull request: its opening with the description, commits,
/// comments, reviews and conversations on lines, and how it ended.
struct ActivityRow: View {
    let entry: PullRequestPage.Entry
    let actions: PullRequestActions

    private static let lineHeight = scaled(17)

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(symbol, size: 12)
                .foregroundStyle(entry.tone == .neutral ? Color.themeMutedForeground : entry.tone.color)
                .frame(width: 18, height: Self.lineHeight)
            VStack(alignment: .leading, spacing: 6) {
                byline
                if !entry.commits.isEmpty {
                    commits
                }
                if let thread = entry.thread {
                    ConversationCard(thread: thread, actions: actions)
                } else if !entry.body.isEmpty {
                    PullRequestTextView(blocks: entry.body)
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .box(in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
                        .padding(.bottom, 2)
                }
                if let subject = entry.subject, entry.thread == nil, !entry.reactions.isEmpty {
                    ReactionBar(reactions: entry.reactions) { kind, on in actions.react(subject, kind, on) }
                }
            }
        }
    }

    private var byline: some View {
        HStack(alignment: .top, spacing: 0) {
            (Text(entry.author).fontWeight(.semibold).foregroundStyle(Color.themeForeground)
                + Text(entry.author.isEmpty ? entry.said : " \(entry.said)").foregroundStyle(Color.themeMutedForeground)
                + Text(" · \(Time.ago(entry.at))").foregroundStyle(Color.themeMutedStrongerForeground))
                .font(.ui(size: 12.5))
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 6)
            HStack(spacing: 0) {
                if let subject = entry.subject, entry.thread == nil, entry.reactions.isEmpty {
                    ReactionPicker(reactions: []) { kind, on in actions.react(subject, kind, on) }
                }
                if entry.kind == "opened", let edit = actions.editDescription {
                    ActionButton(icon: .pencil, help: "Edit the description", pending: actions.working?.key == "menu:body", action: edit)
                }
                if let url = entry.url {
                    ActionButton(icon: .squareArrowOutUpRight, help: "Open on GitHub") { Platform.open(url) }
                }
            }
            .frame(height: Self.lineHeight)
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
                            .foregroundStyle(Color.themePrimary)
                        Text(commit.headline)
                            .font(.ui(size: 12.5))
                            .foregroundStyle(Color.themeForeground)
                            .lineLimit(1)
                            .truncationMode(.tail)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 6)
                    .frame(height: pressable(24))
                }
                .buttonStyle(.highlight(radius: Radius.sm))
                .disabled(commit.sha.isEmpty)
                .help("Show what this commit changed")
            }
        }
        .padding(.leading, -6)
        .padding(.vertical, -3)
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
struct ConversationCard: View {
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
        .box(in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
        .clipShape(RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
    }

    private var header: some View {
        Button {
            open = !isOpen
        } label: {
            HStack(spacing: 6) {
                Image(isOpen ? .chevronDown : .chevronRight, size: 8)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .frame(width: 10)
                Image(FileSymbol.symbol(for: thread.path), size: 11)
                    .foregroundStyle(Color.themeMutedForeground)
                Text(thread.path)
                    .font(.ui(size: 12, weight: .medium))
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(1)
                    .truncationMode(.head)
                if let line = thread.line {
                    Text("line \(line)")
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                        .fixedSize()
                }
                if thread.outdated { Chip("Outdated", tone: .themeWarning).fixedSize() }
                if thread.resolved { Chip("Resolved", tone: .themeSuccess).fixedSize() }
                Spacer(minLength: 0)
                if !isOpen {
                    Text(thread.comments.count == 1 ? "1 comment" : "\(thread.comments.count) comments")
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                }
            }
            .padding(.horizontal, 12)
            .frame(height: pressable(32))
        }
        .buttonStyle(.highlight(radius: 0))
    }

    @ViewBuilder
    private var footer: some View {
        let busy = actions.working != nil
        if replying {
            VStack(alignment: .leading, spacing: 8) {
                TextArea("Reply", text: $reply)
                HStack(spacing: 8) {
                    Spacer()
                    ActionButton("Cancel") {
                        replying = false
                        reply = ""
                    }
                    ActionButton("Reply", variant: .primary, pending: actions.working?.key == "reply:\(thread.id)") {
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
                ActionButton("Reply") { replying = true }
                if let fix = thread.fix {
                    ActionButton("Fix", help: "Have the agent do what it asks") { actions.handOff(fix) }
                }
                Spacer(minLength: 0)
                if thread.canResolve {
                    ActionButton(thread.resolved ? "Unresolve" : "Resolve", pending: actions.working?.key == "resolve:\(thread.id)") {
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
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .padding(.horizontal, 12)
                    .frame(maxWidth: .infinity, minHeight: 18, alignment: .leading)
                    .background(fill(line, lit: index == lines.count - 1))
            }
        }
        .padding(.bottom, 6)
    }

    /// The line's wash, lit on the last line, the one the comment is on.
    private func fill(_ line: String, lit: Bool) -> AnyShapeStyle {
        if line.hasPrefix("+") { return AnyShapeStyle(Color.themeSuccess.wash(lit: lit)) }
        if line.hasPrefix("-") { return AnyShapeStyle(Color.themeDestructive.wash(lit: lit)) }
        return AnyShapeStyle(Color.clear)
    }
}

/// A comment in a conversation on a line.
private struct CommentView: View {
    let comment: PullRequestPage.Comment
    let react: (_ subject: String, _ kind: String, _ on: Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 0) {
                (Text(comment.author).fontWeight(.semibold).foregroundStyle(Color.themeForeground)
                    + Text(" · \(Time.ago(comment.at))").foregroundStyle(Color.themeMutedStrongerForeground))
                    .font(.ui(size: 12.5))
                Spacer(minLength: 4)
                HStack(spacing: 0) {
                    if comment.reactions.isEmpty {
                        ReactionPicker(reactions: [], size: .small) { kind, on in react(comment.id, kind, on) }
                    }
                    if let url = comment.url {
                        ActionButton(icon: .squareArrowOutUpRight, help: "Open on GitHub", size: .small) { Platform.open(url) }
                    }
                }
                .padding(.vertical, -4)
                .padding(.trailing, -6)
            }
            PullRequestTextView(blocks: comment.body)
            if !comment.reactions.isEmpty {
                ReactionBar(reactions: comment.reactions) { kind, on in react(comment.id, kind, on) }
            }
        }
        .padding(12)
    }
}
