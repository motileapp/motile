import SwiftUI

/// A draft or a thread the sidebar's search found, with the message it was found by when its
/// title, its project and its pull request don't have what was searched for.
enum SearchResult: Identifiable {
    case draft(ListedDraft)
    case thread(ThreadInfo, MessageMatch?)

    var id: String {
        switch self {
        case .draft(let listed): "draft:\(listed.id)"
        case .thread(let thread, _): thread.id
        }
    }

    var selection: Selection {
        switch self {
        case .draft(let listed): .draft(listed.id)
        case .thread(let thread, _): .thread(thread.id)
        }
    }

    var thread: ThreadInfo? {
        guard case .thread(let thread, _) = self else { return nil }
        return thread
    }
}

extension AppStore {
    var isSearchingThreads: Bool { !threadSearch.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    /// What the sidebar lists while it searches, in one list: the drafts, the threads found by their
    /// title, project or pull request, active before done, then the ones found by a message. What
    /// the servers find later only ever comes below.
    var searchResults: [SearchResult] {
        let search = threadSearch.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !search.isEmpty else { return [] }
        let threads = activeThreads + doneThreads
        let named = threads.filter { matches($0, search: search) }
        let namedIDs = Set(named.map(\.id))
        let found = messageSearch.matches
        let byMessage = threads.filter { !namedIDs.contains($0.id) && found[$0.id] != nil }
        return searched(listedDrafts, for: search).map(SearchResult.draft)
            + named.map { .thread($0, nil) }
            + byMessage.map { .thread($0, found[$0.id]) }
    }

    /// Opens what the search found and ends the search, then shows it in the sidebar.
    func openSearchResult(_ result: SearchResult) {
        select(result.selection)
        threadSearch = ""
        revealedThreadID = result.thread?.id
    }
}

extension View {
    /// The arrow keys move the highlight through what the search found, round from either end,
    /// and return opens it. It stays on its row while what is found later comes in below. The
    /// keys read what is found and the row as they are pressed: the field keeps an older
    /// closure at times.
    func steersSearch(highlighted: Highlight, open: @escaping (SearchResult) -> Void) -> some View {
        modifier(SearchKeys(highlighted: highlighted, open: open))
    }
}

private struct SearchKeys: ViewModifier {
    @Environment(AppStore.self) private var store
    let highlighted: Highlight
    let open: (SearchResult) -> Void

    func body(content: Content) -> some View {
        content
            .onKeyPress(.downArrow) { steer(1) }
            .onKeyPress(.upArrow) { steer(-1) }
            .onSubmit {
                let results = store.searchResults
                guard results.indices.contains(highlighted.index) else { return }
                open(results[highlighted.index])
            }
            .onChange(of: store.searchResults.map(\.id)) { old, new in
                let kept = old.prefix(highlighted.index + 1)
                highlighted.index = new.prefix(kept.count).elementsEqual(kept) ? highlighted.index : 0
            }
    }

    private func steer(_ step: Int) -> KeyPress.Result {
        let count = store.searchResults.count
        guard count > 0 else { return .ignored }
        highlighted.index = (highlighted.index + step + count) % count
        return .handled
    }
}

/// The line the search shows when it found nothing, in the place of its first row.
struct SearchEmptyLine: View {
    @Environment(AppStore.self) private var store
    let size: CGFloat

    var body: some View {
        HStack(spacing: 6) {
            if store.messageSearch.searching {
                Spinner(size: size)
                Text("Searching messages")
            } else {
                Text("No threads found")
            }
        }
        .font(.ui(size: size))
        .foregroundStyle(Color.themeMutedStrongerForeground)
    }
}

/// What the search found, on two lines that are as tall whatever was found: its project's icon, its
/// title and what it is up to, then the message it was found by, or else its project.
struct SearchResultRow: View {
    @Environment(AppStore.self) private var store
    let result: SearchResult
    /// The arrow keys or the pointer are on it.
    let highlighted: Bool
    /// The keys that open it, while they are held.
    var jump: String?
    var open: (SearchResult) -> Void = { _ in }

    static let height = ThreadRow.topPadding + scaled(22) + scaled(18) + ThreadRow.bottomPadding

    var body: some View {
        let project = project
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                ProjectIcon(project: project, size: 14)
                    .opacity(.disabled, when: isDone && !highlighted)
                Text(title)
                    .font(.ui(size: 13, weight: .medium))
                    .lineLimit(1)
                    .foregroundStyle(highlighted ? Color.themeForeground : isDone ? Color.themeMutedStrongerForeground : Color.themeMutedForeground)
                    .layoutPriority(1)
                Spacer(minLength: 6)
                HStack(spacing: 6) {
                    status(project: project)
                }
                .fixedSize()
            }
            .frame(height: scaled(22))
            detail(project: project)
                .font(.ui(size: 12))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .frame(height: scaled(18))
        }
        .padding(.horizontal, ThreadRow.sidePadding)
        .padding(.top, ThreadRow.topPadding)
        .padding(.bottom, ThreadRow.bottomPadding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(height: Self.height)
        .padding(rowMargin)
        #if os(macOS)
        // The table the row is in takes the click.
        .contentShape(Rectangle())
        .hoverHighlight(radius: Radius.md, inset: rowMargin, hovered: highlighted)
        #else
        .button(.highlight(radius: Radius.md, lit: highlighted, inset: rowMargin)) { open(result) }
        #endif
    }

    private var project: Project? {
        switch result {
        case .draft(let listed): store.project(listed.draft.projectID)
        case .thread(let thread, _): store.project(thread.projectID)?.seen(from: thread)
        }
    }

    private var title: String {
        switch result {
        case .draft(let listed): listed.preview
        case .thread(let thread, _): thread.title
        }
    }

    private var isDone: Bool { result.thread?.isDone == true }

    @ViewBuilder private func status(project: Project?) -> some View {
        switch result {
        case .draft:
            HStack(spacing: 3) {
                Image(.file, size: 11)
                Text("Draft")
                    .font(.ui(size: 11, weight: .medium))
            }
            .foregroundStyle(Color.themeMutedForeground)
        case .thread(let thread, _):
            if let pullRequest = project?.pullRequest(of: thread) {
                PullRequestLabel(pullRequest: pullRequest, colored: !thread.isDone)
            }
            if let jump {
                KeyCaps([jump])
                    .foregroundStyle(Color.themeForeground)
            } else if thread.isDone {
                Text(Time.ago(thread.doneAt ?? thread.updatedAt, now: AgoClock.shared.now))
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
            } else {
                ThreadStatus(thread: thread)
            }
        }
    }

    @ViewBuilder private func detail(project: Project?) -> some View {
        if case .thread(_, let found?) = result {
            Text(Self.excerpt(found))
        } else {
            Text(project?.name ?? result.thread.map { URL(fileURLWithPath: $0.cwd).lastPathComponent } ?? "No project")
                .foregroundStyle(Color.themeMutedStrongerForeground)
        }
    }

    /// Who wrote the message, then the message with what was searched for set off.
    private static func excerpt(_ found: MessageMatch) -> AttributedString {
        var line = AttributedString(found.byUser ? "You: " : "Agent: ")
        line.foregroundColor = .themeMutedForeground
        line.font = .ui(size: 12, weight: .medium)
        for part in found.parts {
            var piece = AttributedString(part.text)
            piece.foregroundColor = part.matched ? .themeForeground : .themeMutedStrongerForeground
            if part.matched { piece.font = .ui(size: 12, weight: .semibold) }
            line += piece
        }
        return line
    }
}
