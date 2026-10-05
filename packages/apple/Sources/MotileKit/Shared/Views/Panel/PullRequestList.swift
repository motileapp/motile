import SwiftUI

/// The repository's pull requests, the last updated first, to open one in a tab of its own or link
/// it to the thread.
struct PullRequestListSurface: View {
    @Environment(AppStore.self) private var store
    let target: PanelTarget
    @AppStorage("pullRequests.state") private var state = "open"
    @State private var search = ""
    @State private var linking = ""
    @State private var asked = 0

    private static let states = [("open", "Open"), ("merged", "Merged"), ("closed", "Closed"), ("all", "All")]

    var body: some View {
        let panel = store.sidePanel
        VStack(spacing: 0) {
            PanelBar { bar }
            if let notice = panel.pullRequestNotice {
                PullRequestNoticeBar(notice: notice)
            }
            if let reason = store.pullRequestsUnavailable {
                PanelMessage(text: reason)
            } else if !store.pullRequestsExtended {
                PanelMessage(text: "Update your server to see its pull requests here.")
            } else {
                content
                    .task(id: PanelTrigger(target: target, path: state, version: store.workspaceVersion, asked: asked)) {
                        panel.loadPullRequests(of: target, state: state)
                    }
            }
        }
    }

    @ViewBuilder
    private var bar: some View {
        Menu {
            ForEach(Self.states, id: \.0) { choice in
                Toggle(choice.1, isOn: Binding { state == choice.0 } set: { _ in state = choice.0 })
            }
        } label: {
            HStack(spacing: 5) {
                Text(Self.states.first { $0.0 == state }?.1 ?? "Open")
                    .font(.ui(size: 12.5, weight: .medium))
                Image(.chevronDown, size: 8)
                    .foregroundStyle(Color.themeTertiary)
            }
            .foregroundStyle(Color.themeText)
            .padding(.horizontal, 8)
            .frame(height: pressable(26))
            .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .hoverHighlight()
        .padding(.leading, -8)
        .help("Which pull requests to show")
        HStack(spacing: 5) {
            Image(.search, size: 11)
                .foregroundStyle(Color.themeTertiary)
            TextField("Search", text: $search)
                .textFieldStyle(.plain)
                .font(.ui(size: 12.5))
        }
        .padding(.horizontal, 8)
        .frame(height: scaled(26))
        .background(Color.themeHover, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
        .frame(maxWidth: 220)
        Spacer(minLength: 4)
        IconOnlyButton(symbol: .rotateCw, help: "Read the pull requests again") { asked += 1 }
    }

    @ViewBuilder
    private var content: some View {
        switch store.sidePanel.pullRequestList {
        case .loading:
            PanelLoading()
        case .failed(let message):
            PanelMessage(text: message, failed: true)
        case .ready(let rows):
            let shown = filtered(rows)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    if let thread = store.selectedThread {
                        LinkPullRequestField(text: $linking) { number in
                            store.sidePanel.link(number, thread: thread.id, serverID: thread.serverID)
                        }
                        .frame(maxWidth: 380, alignment: .leading)
                        .padding(.horizontal, 6)
                        .padding(.bottom, 8)
                    }
                    if shown.isEmpty {
                        Text(search.isEmpty ? "No \(state == "all" ? "" : "\(state) ")pull requests." : "None match “\(search)”.")
                            .font(.ui(size: 12.5))
                            .foregroundStyle(Color.themeSecondary)
                            .frame(maxWidth: .infinity)
                            .padding(.top, 40)
                    }
                    ForEach(shown) { row in
                        PullRequestListRow(row: row, target: target, linked: store.selectedThread?.pullRequest?.number == row.number)
                    }
                }
                .padding(8)
            }
        }
    }

    private func filtered(_ rows: [PullRequestRow]) -> [PullRequestRow] {
        let words = search.trimmingCharacters(in: .whitespaces).lowercased()
        guard !words.isEmpty else { return rows }
        return rows.filter { row in
            "\(row.number) \(row.title) \(row.author) \(row.head)".lowercased().contains(words)
        }
    }
}

private struct PullRequestListRow: View {
    @Environment(AppStore.self) private var store
    let row: PullRequestRow
    let target: PanelTarget
    /// It is the thread's own.
    let linked: Bool

    var body: some View {
        Button {
            store.sidePanel.showPullRequest(row.number, of: target)
        } label: {
            HStack(alignment: .top, spacing: 10) {
                Image(row.state.symbol, size: 13)
                    .foregroundStyle(row.state.color)
                    .frame(width: 16, height: scaled(18))
                VStack(alignment: .leading, spacing: 3) {
                    HStack(alignment: .firstTextBaseline, spacing: 6) {
                        Text(row.title)
                            .font(.ui(size: 13, weight: .medium))
                            .foregroundStyle(Color.themeText)
                            .lineLimit(2)
                            .multilineTextAlignment(.leading)
                        if linked {
                            Text("This thread")
                                .font(.ui(size: 10.5, weight: .semibold))
                                .foregroundStyle(Color.themeLink)
                                .padding(.horizontal, 6)
                                .frame(height: scaled(17))
                                .background(Color.themeLink.opacity(0.14), in: Capsule())
                                .fixedSize()
                        }
                        Spacer(minLength: 4)
                        signals
                    }
                    (Text(verbatim: "#\(row.number)") + Text(" · \(row.author) · \(row.head) → \(row.base) · \(Time.ago(row.updatedAt))"))
                        .font(.ui(size: 11.5))
                        .foregroundStyle(Color.themeTertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.highlight())
        .contextMenu {
            if let thread = store.selectedThread, !linked {
                Button("Link to This Thread") { store.sidePanel.link(row.number, thread: thread.id, serverID: thread.serverID) }
            }
            if let url = row.url {
                Button("Open on GitHub") { Platform.open(url) }
                Button("Copy Link") { Platform.copy(url.absoluteString) }
            }
        }
    }

    /// How its checks and its review went, and what it changes.
    private var signals: some View {
        HStack(spacing: 7) {
            if let review = row.review {
                Image(review.tone == .success ? .circleCheck : review.tone == .danger ? .circleAlert : .eye, size: 11)
                    .foregroundStyle(review.tone.color)
                    .help(review.label)
            }
            if let checks = row.checks {
                Circle()
                    .fill(checks.color)
                    .frame(width: 7, height: 7)
                    .help(row.checksLabel ?? "")
            }
            if row.additions + row.deletions > 0 {
                Text(AttributedString(LineCountText.text(added: row.additions, removed: row.deletions)))
                    .font(.ui(size: 11.5))
            }
        }
        .fixedSize()
    }
}
