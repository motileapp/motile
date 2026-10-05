import SwiftUI

extension ButtonVariant {
    /// The look of a button as the core names it.
    init(style: String) {
        switch style {
        case "primary": self = .primary
        case "danger": self = .danger
        default: self = .secondary
        }
    }
}

struct BranchName: View {
    let name: String

    var body: some View {
        Chip(name, monospaced: true)
            .truncationMode(.middle)
            .textSelection(.enabled)
    }
}

/// A reviewer, with a dot in the colour of their latest verdict.
struct ReviewerChip: View {
    let reviewer: PullRequestPage.Reviewer

    var body: some View {
        Chip(reviewer.name, dot: reviewer.tone == .neutral ? Color.themeTertiary : reviewer.tone.color)
            .help("\(reviewer.name): \(reviewer.label)")
    }
}

/// The reactions to a comment or a review, each one a toggle, and a menu to add another.
struct ReactionBar: View {
    let reactions: [PullRequestPage.Reaction]
    /// Has the user react so, or take it back.
    let react: (String, Bool) -> Void

    var body: some View {
        HStack(spacing: 4) {
            ForEach(reactions) { reaction in
                ActionButton(
                    "\(reaction.emoji) \(reaction.count)", help: reaction.mine ? "Take your reaction back" : "React so too",
                    variant: reaction.mine ? .accent : .secondary, size: .small, round: true
                ) {
                    react(reaction.kind, !reaction.mine)
                }
                .monospacedDigit()
            }
            ReactionPicker(reactions: reactions, size: .small, react: react)
        }
    }
}

/// The menu of GitHub's reactions, to add one or take it back.
struct ReactionPicker: View {
    let reactions: [PullRequestPage.Reaction]
    var size = ControlSize.regular
    let react: (String, Bool) -> Void

    var body: some View {
        ActionMenu(icon: .smilePlus, help: "Add a reaction", size: size) {
            ForEach(PullRequestPage.Reaction.all, id: \.kind) { choice in
                let mine = reactions.first { $0.kind == choice.kind }?.mine ?? false
                Button("\(choice.emoji)  \(mine ? "Take Back" : "React")") { react(choice.kind, !mine) }
            }
        }
    }
}

/// What the last action did or why it couldn't, under the tab's bar.
struct PullRequestNoticeBar: View {
    @Environment(AppStore.self) private var store
    let notice: PullRequestNotice

    private static let lineHeight = scaled(17)

    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .top, spacing: 8) {
                Image(notice.failed ? .circleAlert : .circleCheck, size: 13)
                    .foregroundStyle(notice.failed ? Color.themeDanger : Color.themeSuccess)
                    .frame(height: Self.lineHeight)
                Text(notice.text)
                    .font(.ui(size: 12.5))
                    .foregroundStyle(Color.themeText)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(minHeight: Self.lineHeight)
                Spacer(minLength: 4)
                HStack(spacing: 2) {
                    if let url = notice.url {
                        ActionButton("Open", variant: .link, size: .small) { Platform.open(url) }
                    }
                    ActionButton(icon: .x, help: "Close", size: .small) { store.sidePanel.dismissPullRequestNotice() }
                }
                .frame(height: Self.lineHeight)
            }
            .padding(.leading, 12)
            .padding(.trailing, 6)
            .padding(.vertical, 8)
            .background(notice.failed ? Color.themeDanger.opacity(0.08) : Color.themeSuccess.opacity(0.08))
            PanelLine()
        }
    }
}

/// A box to write in, with a word in it while it is empty.
struct WritingField: View {
    @Binding var text: String
    let placeholder: String
    /// Without one it is as tall as there is room for.
    var height: CGFloat? = 84

    var body: some View {
        TextEditor(text: $text)
            .font(.ui(size: 12.5))
            .scrollContentBackground(.hidden)
            .padding(.horizontal, 4)
            .padding(.vertical, 6)
            .frame(minHeight: height, maxHeight: height ?? .infinity)
            .background(Color.themeField, in: RoundedRectangle(cornerRadius: Radius.control, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: Radius.control, style: .continuous).strokeBorder(Color.themeBorder, lineWidth: 1)
            }
            .overlay(alignment: .topLeading) {
                if text.isEmpty {
                    Text(placeholder)
                        .font(.ui(size: 12.5))
                        .foregroundStyle(Color.themeTertiary)
                        .padding(.horizontal, 9)
                        .padding(.vertical, 6)
                        .allowsHitTesting(false)
                }
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
                        .background(Color(platform: Theme.codeBlock), in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
                }
            }
        }
    }
}

/// The sheet that edits the description, with how it will look.
struct DescriptionEditor: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    let original: String
    let save: (String) -> Void
    @State private var text = ""
    @State private var previewing = false
    @State private var preview: [PullRequestText] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Edit description")
                    .font(.ui(size: 15, weight: .semibold))
                Spacer()
                Picker("", selection: $previewing) {
                    Text("Write").tag(false)
                    Text("Preview").tag(true)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .fixedSize()
            }
            if previewing {
                ScrollView {
                    if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Text("Nothing to preview")
                            .font(.ui(size: 12.5))
                            .foregroundStyle(Color.themeTertiary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    } else {
                        PullRequestTextView(blocks: preview)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .padding(12)
                .frame(maxHeight: .infinity)
                .background(Color.themeBubble, in: RoundedRectangle(cornerRadius: Radius.control, style: .continuous))
            } else {
                WritingField(text: $text, placeholder: "Say what this changes and why", height: nil)
            }
            HStack(spacing: 8) {
                Spacer()
                ActionButton("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary) {
                    save(text)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(text == original)
            }
        }
        .padding(20)
        #if os(macOS)
        .frame(width: 560, height: 440)
        #else
        .presentationDetents([.large])
        #endif
        .onAppear { text = original }
        .task(id: previewing) {
            guard previewing else { return }
            store.core.send("markdown", ["text": text], read: { PullRequestText.blocks($0.objects("blocks")) }) { result in
                if case .success(let blocks) = result { preview = blocks }
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
