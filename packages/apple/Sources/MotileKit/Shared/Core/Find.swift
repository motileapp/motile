import SwiftUI

/// The matches finding in the thread found in one row, in UTF-16 units, and which of them is the
/// current one.
struct FoundMarks: Equatable {
    let ranges: [NSRange]
    let current: Int?

    init(ranges: [NSRange], current: Int?) {
        self.ranges = ranges
        self.current = current
    }

    init(json: JSON) {
        ranges = (json["ranges"] as? [[NSNumber]] ?? []).compactMap { pair in
            guard pair.count == 2 else { return nil }
            return NSRange(location: pair[0].intValue, length: pair[1].intValue)
        }
        current = (json["current"] as? NSNumber)?.intValue
    }
}

/// Where finding in the thread starts: the first row that shows below the bars, and the first
/// character of its first whole line there, in UTF-16 units of the text the core sent. A long
/// reply is a single row, so its first match can be far above what shows.
struct FindFrom: Equatable {
    let rowID: String
    let offset: Int

    var json: [String: Any] { ["row_id": rowID, "offset": offset] }
}

/// Finding in the open thread, as a browser finds in a page: the bar's query, and what the core
/// found for it.
@Observable
final class FindState {
    /// Whether the bar shows.
    private(set) var shown = false
    /// Whether the transcript shows what was found: with the bar, or alone after the sidebar's
    /// search opened a thread at a message.
    private(set) var active = false
    var query = ""
    private(set) var count = 0
    private(set) var current: Int?
    /// Whether the whole thread has been searched.
    private(set) var ready = true
    /// Asks the bar's field to take the keys and select what it has.
    private(set) var focusRequests = 0
    /// Counts each lighting and closing, so that a fade only ends the lighting it was set for.
    fileprivate var lit = 0

    /// How long what the sidebar's search found stays lit in the thread it opened.
    static let litFor: TimeInterval = 5

    /// "3/41", or "0/0" when nothing matches.
    var label: String {
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else { return "" }
        guard let current, count > 0 else { return "0/0" }
        return "\(current + 1)/\(count)"
    }

    fileprivate func open(query: String?) {
        if let query { self.query = query }
        shown = true
        active = true
        focusRequests += 1
    }

    fileprivate func light(query: String) {
        self.query = query
        active = true
        lit += 1
    }

    fileprivate func close() {
        lit += 1
        shown = false
        active = false
        count = 0
        current = nil
        ready = true
    }

    fileprivate func found(count: Int, current: Int?, ready: Bool) {
        if self.count != count { self.count = count }
        if self.current != current { self.current = current }
        if self.ready != ready { self.ready = ready }
    }
}

extension AppStore {
    /// Opens the bar, with `query` in it when given: the selected text, or what the sidebar searched.
    /// It finds from the message `itemID` when that has a match.
    func openFind(query: String? = nil, itemID: String? = nil) {
        guard let threadID = transcript.threadID, selectedThread != nil else { return }
        find.open(query: query)
        sendFind(threadID: threadID, itemID: itemID)
    }

    /// Lights what the sidebar searched for in the open thread, without the bar, and scrolls to
    /// it in the message `itemID`, or to the message when the words are apart there. A server
    /// that doesn't say the message has it scroll to the first match. It fades after a while
    /// unless the bar opened on it; ⌘F opens the bar on it, Esc puts it away.
    func lightFound(_ query: String, itemID: String?) {
        guard let threadID = transcript.threadID, selectedThread != nil, !query.isEmpty else { return }
        find.light(query: query)
        let lit = find.lit
        sendFind(threadID: threadID, itemID: itemID, fromViewport: false)
        DispatchQueue.main.asyncAfter(deadline: .now() + FindState.litFor) { [weak self] in
            guard let self, self.find.lit == lit, !self.find.shown else { return }
            self.closeFind()
        }
    }

    func closeFind() {
        guard find.active else { return }
        find.close()
        transcript.setFound([:])
        guard let threadID = transcript.threadID else { return }
        core.send("find_end", ["thread_id": threadID])
    }

    func setFindQuery(_ query: String) {
        guard find.query != query else { return }
        find.query = query
        guard let threadID = transcript.threadID else { return }
        sendFind(threadID: threadID, itemID: nil)
    }

    /// Moves to the next match, or with `back` the one before. The bar opens first when it is closed.
    func stepFind(back: Bool) {
        guard let threadID = transcript.threadID, selectedThread != nil else { return }
        guard find.shown else { return openFind() }
        core.send("find_step", ["thread_id": threadID, "back": back])
    }

    private func sendFind(threadID: String, itemID: String?, fromViewport: Bool = true) {
        var fields: [String: Any] = ["thread_id": threadID, "query": find.query]
        if fromViewport, let from = transcript.findFrom(cover: FindBar.cover) { fields["from"] = from.json }
        if let itemID { fields["item_id"] = itemID }
        core.send("find", fields)
    }

    /// What the core found, and the matches in the rows the transcript has.
    func takeFound(_ event: JSON) -> (() -> Void)? {
        let threadID = event.string("thread_id")
        let query = event.string("query")
        let (count, current, ready) = (event.int("count"), (event["current"] as? NSNumber)?.intValue, event.bool("ready"))
        let marks = Dictionary(event.objects("rows").map { ($0.string("row_id"), FoundMarks(json: $0)) }, uniquingKeysWith: { first, _ in first })
        return { [weak self] in
            guard let self, self.find.active, self.transcript.threadID == threadID, self.find.query == query else { return }
            self.find.found(count: count, current: current, ready: ready)
            self.transcript.setFound(marks)
        }
    }

    func takeReveal(_ event: JSON) -> (() -> Void)? {
        let (threadID, rowID) = (event.string("thread_id"), event.string("row_id"))
        let range = (event["range"] as? [NSNumber]).flatMap { $0.count == 2 ? NSRange(location: $0[0].intValue, length: $0[1].intValue) : nil }
        return { [weak self] in
            guard let self, self.transcript.threadID == threadID else { return }
            self.transcript.reveal(rowID: rowID, range: range)
        }
    }
}
