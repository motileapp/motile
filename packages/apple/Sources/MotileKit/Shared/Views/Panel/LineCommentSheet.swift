import SwiftUI

/// A line of a pull request's diff that a comment is being written on.
struct CommentedLine: Identifiable {
    let path: String
    /// As GitHub counts it, in the file as it was or as it is.
    let line: Int
    /// "left" or "right".
    let side: String
    let code: String

    var id: String { "\(side):\(path):\(line)" }
}

/// The sheet that comments on a line: what was said there already, to reply to, and a comment
/// to keep for the review or to hand to the agent.
struct LineCommentSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let target: PanelTarget
    let page: PullRequestPage
    let commented: CommentedLine
    @State private var text = ""

    private var threads: [PullRequestPage.Thread] {
        page.threads.filter { $0.path == commented.path && $0.line == commented.line && $0.side == commented.side }
    }

    private var written: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Image(FileSymbol.symbol(for: commented.path), size: 12)
                        .foregroundStyle(Color.themeSecondary)
                    Text(commented.path)
                        .font(.ui(size: 13, weight: .semibold))
                        .lineLimit(1)
                        .truncationMode(.head)
                    Text("line \(commented.line)")
                        .font(.ui(size: 12.5))
                        .foregroundStyle(Color.themeTertiary)
                        .fixedSize()
                }
                Text(commented.code.isEmpty ? " " : commented.code)
                    .font(.ui(size: 12, design: .monospaced))
                    .foregroundStyle(Color.themeText)
                    .lineLimit(3)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .layered(in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            }
            if !threads.isEmpty {
                ScrollView {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(threads) { thread in
                            ForEach(thread.comments) { comment in
                                VStack(alignment: .leading, spacing: 4) {
                                    (Text(comment.author).fontWeight(.semibold).foregroundStyle(Color.themeText)
                                        + Text(" · \(Time.ago(comment.at))").foregroundStyle(Color.themeTertiary))
                                        .font(.ui(size: 12.5))
                                    PullRequestTextView(blocks: comment.body)
                                }
                            }
                        }
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 220)
                .layered(in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
            }
            TextArea(threads.isEmpty ? "Comment on this line" : "Reply, or say something new", text: $text, lines: 5)
            #if os(macOS)
            HStack(spacing: 8) {
                ActionButton("Ask the Agent", help: "Put this line and what you wrote in the composer", action: askAgent)
                    .disabled(written.isEmpty)
                Spacer()
                ActionButton("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                if let thread = threads.last {
                    ActionButton("Reply") { replyTo(thread) }
                        .disabled(written.isEmpty)
                }
                ActionButton("Add to Review", variant: .primary, action: addToReview)
                    .keyboardShortcut(.defaultAction)
                    .disabled(written.isEmpty)
            }
            #else
            VStack(spacing: 8) {
                ActionButton("Add to Review", variant: .primary, size: .large, fills: true, action: addToReview)
                    .disabled(written.isEmpty)
                if let thread = threads.last {
                    ActionButton("Reply", size: .large, fills: true) { replyTo(thread) }
                        .disabled(written.isEmpty)
                }
                ActionButton("Ask the Agent", size: .large, fills: true, action: askAgent)
                    .disabled(written.isEmpty)
                ActionButton("Cancel", variant: .ghost, size: .large, fills: true) { dismiss() }
            }
            #endif
        }
        .padding(20)
        #if os(macOS)
        .frame(width: 500)
        #else
        .frame(maxHeight: .infinity, alignment: .top)
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        #endif
    }

    private func addToReview() {
        let comment = PendingLineComment(path: commented.path, line: commented.line, side: commented.side, body: written)
        store.sidePanel.addPending(comment, to: page.number)
        dismiss()
    }

    private func replyTo(_ thread: PullRequestPage.Thread) {
        let edit: JSON = ["kind": "reply", "thread": thread.id, "body": written]
        store.sidePanel.edit(edit, on: target, number: page.number)
        dismiss()
    }

    private func askAgent() {
        let fields: JSON = [
            "number": page.number, "url": page.url?.absoluteString ?? "", "head": page.head, "path": commented.path,
            "line": commented.line, "code": commented.code, "note": written,
        ]
        store.core.send("line_prompt", fields) { result in
            guard case .success(let answer) = result else { return }
            store.handOff(answer.string("prompt"))
        }
        dismiss()
    }
}
