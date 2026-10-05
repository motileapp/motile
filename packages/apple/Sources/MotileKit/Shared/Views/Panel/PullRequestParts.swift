import SwiftUI

/// A button of the pull request's tab, in the panel's colours: filled for what the state calls
/// for, red for what fixes it, quiet for the rest. While its action runs it turns, and says so.
struct PullRequestActionButton: View {
    @Environment(\.isEnabled) private var isEnabled
    let label: String
    let style: String
    /// The method menu sits right of it, so its right corners are square.
    var joined = false
    /// What it says while its action runs: "Merging…".
    var working: String?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if working != nil {
                    Spinner(color: foreground)
                        .frame(width: 11, height: 11)
                }
                Text(working ?? label)
                    .font(.ui(size: 12, weight: .medium))
                    .lineLimit(1)
            }
            .foregroundStyle(foreground)
            .padding(.horizontal, 11)
            .frame(height: scaled(28))
            .background(background, in: shape)
            .overlay {
                if style == "danger" { shape.strokeBorder(Color.themeDanger.opacity(0.6), lineWidth: 1) }
            }
        }
        .buttonStyle(DimButtonStyle())
        .opacity(isEnabled || working != nil ? 1 : 0.45)
        .fixedSize()
        .animation(.easeOut(duration: 0.15), value: working)
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

/// A ring that turns, in any colour, for what is under way.
struct Spinner: View {
    let color: Color
    @State private var turned = false

    var body: some View {
        Circle()
            .trim(from: 0.15, to: 1)
            .stroke(color, style: StrokeStyle(lineWidth: 1.6, lineCap: .round))
            .rotationEffect(.degrees(turned ? 360 : 0))
            .onAppear {
                withAnimation(.linear(duration: 0.8).repeatForever(autoreverses: false)) { turned = true }
            }
    }
}

/// The pull request's state, as a label in its colour.
struct StateLabel: View {
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

struct BranchName: View {
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

/// A label of the pull request, in GitHub's colour for it.
struct LabelChip: View {
    let name: String
    let color: Color

    var body: some View {
        HStack(spacing: 5) {
            Circle()
                .fill(color)
                .frame(width: 7, height: 7)
            Text(name)
                .font(.ui(size: 11.5, weight: .medium))
                .foregroundStyle(Color.themeText)
                .lineLimit(1)
        }
        .padding(.horizontal, 7)
        .frame(height: scaled(20))
        .background(color.opacity(0.16), in: Capsule())
        .overlay { Capsule().strokeBorder(color.opacity(0.35), lineWidth: 1) }
    }
}

/// A reviewer, with a dot in the colour of their latest verdict.
struct ReviewerChip: View {
    let reviewer: PullRequestPage.Reviewer

    var body: some View {
        HStack(spacing: 5) {
            Circle()
                .fill(reviewer.tone == .neutral ? Color.themeTertiary : reviewer.tone.color)
                .frame(width: 7, height: 7)
            Text(reviewer.name)
                .font(.ui(size: 11.5, weight: .medium))
                .foregroundStyle(Color.themeText)
                .lineLimit(1)
        }
        .padding(.horizontal, 7)
        .frame(height: scaled(20))
        .background(Color.themeHover, in: Capsule())
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
                Button {
                    react(reaction.kind, !reaction.mine)
                } label: {
                    HStack(spacing: 4) {
                        Text(reaction.emoji)
                            .font(.system(size: 11.5))
                        Text(verbatim: "\(reaction.count)")
                            .font(.ui(size: 11.5, weight: .medium))
                            .monospacedDigit()
                            .foregroundStyle(reaction.mine ? Color.themeLink : Color.themeSecondary)
                    }
                    .padding(.horizontal, 7)
                    .frame(height: scaled(22))
                    .background(reaction.mine ? Color.themeLink.opacity(0.14) : Color.themeHover, in: Capsule())
                    .overlay { Capsule().strokeBorder(reaction.mine ? Color.themeLink.opacity(0.45) : Color.clear, lineWidth: 1) }
                    .contentShape(Capsule())
                }
                .buttonStyle(DimButtonStyle())
                .help(reaction.mine ? "Take your reaction back" : "React so too")
            }
            ReactionPicker(reactions: reactions, react: react)
        }
    }
}

/// The menu of GitHub's reactions, to add one or take it back.
struct ReactionPicker: View {
    let reactions: [PullRequestPage.Reaction]
    let react: (String, Bool) -> Void

    var body: some View {
        Menu {
            ForEach(PullRequestPage.Reaction.all, id: \.kind) { choice in
                let mine = reactions.first { $0.kind == choice.kind }?.mine ?? false
                Button("\(choice.emoji)  \(mine ? "Take Back" : "React")") { react(choice.kind, !mine) }
            }
        } label: {
            Image(.smilePlus, size: 12)
                .foregroundStyle(Color.themeTertiary)
                .frame(width: scaled(26), height: scaled(22))
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight(radius: 11)
        .help("Add a reaction")
    }
}

/// What the last action did or why it couldn't, under the tab's bar.
struct PullRequestNoticeBar: View {
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
            .background(Color.themeField, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 8, style: .continuous).stroke(Color.themeStrongBorder, lineWidth: 1)
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
                        .background(Color(platform: Theme.codeBlock), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
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
                .background(Color.themeBubble, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            } else {
                WritingField(text: $text, placeholder: "Say what this changes and why", height: nil)
            }
            HStack(spacing: 8) {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
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
