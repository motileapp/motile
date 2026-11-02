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
    @Environment(\.closeModal) private var close
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
                    Image(FileSymbol.symbol(for: commented.path), size: .md)
                        .foregroundStyle(Color.themeMutedForeground)
                    Text(commented.path)
                        .font(.ui(size: 13, weight: .semibold))
                        .lineLimit(1)
                        .truncationMode(.head)
                    Text("line \(commented.line)")
                        .font(.ui(size: 12.5))
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                        .fixedSize()
                }
                Text(commented.code.isEmpty ? " " : commented.code)
                    .font(.ui(size: 12, design: .monospaced))
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(3)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .box(in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
            }
            if !threads.isEmpty {
                ScrollView {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(threads) { thread in
                            ForEach(thread.comments) { comment in
                                VStack(alignment: .leading, spacing: 4) {
                                    (Text(comment.author).fontWeight(.semibold).foregroundStyle(Color.themeForeground)
                                        + Text(" · \(Time.ago(comment.at))").foregroundStyle(Color.themeMutedStrongerForeground))
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
                .box(in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
            }
            TextArea(threads.isEmpty ? "Comment on this line" : "Reply, or say something new", text: $text, lines: 5)
            #if os(macOS)
            HStack(spacing: 0) {
                ActionButton("Ask the Agent", help: "Put this line and what you wrote in the composer", margin: .beside(trailing: 8), action: askAgent)
                    .disabled(written.isEmpty)
                Spacer()
                ActionButton("Cancel", margin: .beside(leading: 8, trailing: 4)) { close() }
                    .keyboardShortcut(.cancelAction)
                if let thread = threads.last {
                    ActionButton("Reply", margin: .beside(leading: 4, trailing: 4)) { replyTo(thread) }
                        .disabled(written.isEmpty)
                }
                ActionButton("Add to Review", variant: .primary, margin: .beside(leading: 4), action: addToReview)
                    .keyboardShortcut(.defaultAction)
                    .disabled(written.isEmpty)
            }
            #else
            let count = threads.isEmpty ? 3 : 4
            VStack(spacing: 0) {
                ActionButton(
                    "Add to Review", variant: .primary, size: .large, fills: true, margin: .sharing(8, at: 0, of: count, along: .vertical),
                    action: addToReview
                )
                .disabled(written.isEmpty)
                if let thread = threads.last {
                    ActionButton("Reply", size: .large, fills: true, margin: .sharing(8, at: 1, of: count, along: .vertical)) { replyTo(thread) }
                        .disabled(written.isEmpty)
                }
                ActionButton("Ask the Agent", size: .large, fills: true, margin: .sharing(8, at: count - 2, of: count, along: .vertical), action: askAgent)
                    .disabled(written.isEmpty)
                ActionButton("Cancel", variant: .ghost, size: .large, fills: true, margin: .sharing(8, at: count - 1, of: count, along: .vertical)) {
                    close()
                }
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
        store.rightPanel.addPending(comment, to: page.number)
        close()
    }

    private func replyTo(_ thread: PullRequestPage.Thread) {
        let edit: JSON = ["kind": "reply", "thread": thread.id, "body": written]
        store.rightPanel.edit(edit, on: target, number: page.number)
        close()
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
        close()
    }
}
