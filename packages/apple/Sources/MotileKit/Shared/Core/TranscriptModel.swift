import Foundation

/// Where a transcript was left: the row at the top of its viewport, and how far the viewport's
/// top is below the row's.
struct TranscriptPlace: Equatable {
    let rowID: String
    let delta: CGFloat

    init(rowID: String, delta: CGFloat) {
        self.rowID = rowID
        self.delta = delta
    }

    init?(json: [String: Any]?) {
        guard let json, let rowID = json["row_id"] as? String else { return nil }
        self.init(rowID: rowID, delta: CGFloat((json["delta"] as? NSNumber)?.doubleValue ?? 0))
    }

    var json: [String: Any] { ["row_id": rowID, "delta": Double(delta)] }
}

/// The rows of the open thread, kept in step with the core, and passed on to the view that draws
/// them. The view registers itself with `attach`; until it has, the rows wait here.
final class TranscriptModel {
    struct Hooks {
        /// The view shows the rows at the place, or at the end without one.
        let reset: ([RowModel], TranscriptPlace?) -> Void
        /// Where the view shows the rows, or nil at the end.
        let place: () -> TranscriptPlace?
        /// The view keeps what it shows until the next reset.
        let keep: () -> Void
        let splice: (Int, Int, [RowModel]) -> Void
        let pending: (RowModel?) -> Void
        let activity: (Activity) -> Void
        let recolor: (String, CodeContent) -> Void
        /// Whether the thread has turns before the first row.
        let earlier: (Bool) -> Void
        /// Whether the thread has caught up with its server.
        let live: (Bool) -> Void
        /// Where finding starts in what the view shows below the bars, the find bar's `cover` too.
        let findFrom: (_ cover: CGFloat) -> FindFrom?
        /// What finding in the thread found in the rows, by row.
        let found: ([String: FoundMarks]) -> Void
        /// The view scrolls to the row, and to that part of its text.
        let reveal: (String, NSRange?) -> Void
    }

    private(set) var threadID: String?
    private(set) var rows: [RowModel] = []
    /// Where the rows are shown when a view next shows them.
    private var place: TranscriptPlace?
    private(set) var activity = Activity()
    private var hasEarlier = false
    private(set) var live = false
    /// The view still shows the rows of the thread that was open before.
    private var keepsRows = false
    private var pending: RowModel?
    /// The text of the message on its way.
    private var pendingText: String?
    private var pendingSkills: [SkillSpan] = []
    private var pendingCollapsible = false
    private var hooks: Hooks?
    private var owner: ObjectIdentifier?
    private(set) var found: [String: FoundMarks] = [:]
    /// The row to scroll to once a view shows the rows.
    private var revealing: (String, NSRange?)?

    var isEmpty: Bool { rows.isEmpty && pending == nil }
    var hasPending: Bool { pending != nil }

    /// The turns that changed files, the first one first.
    var turns: [TurnChange] {
        rows.compactMap { row in
            guard case .changes(let content) = row.kind else { return nil }
            return TurnChange(id: row.itemID, at: content.at, files: content.files)
        }
    }

    func attach(_ hooks: Hooks, owner: AnyObject) {
        self.hooks = hooks
        self.owner = ObjectIdentifier(owner)
        hooks.live(live)
        hooks.reset(rows, place)
        hooks.pending(pending)
        hooks.activity(activity)
        hooks.earlier(hasEarlier)
        hooks.found(found)
        if let (rowID, range) = revealing {
            revealing = nil
            hooks.reveal(rowID, range)
        }
    }

    /// Lets go of the view, unless another one has attached since.
    func detach(owner: AnyObject) {
        guard self.owner == ObjectIdentifier(owner) else { return }
        if !keepsRows { place = hooks?.place() }
        hooks = nil
        self.owner = nil
    }

    /// Starts showing another thread, or none. Its rows follow from the core. With `keepingRows`
    /// the view shows the rows it has until they do, so that it isn't empty in between.
    func begin(threadID: String?, live: Bool = false, keepingRows: Bool = false) {
        self.threadID = threadID
        rows = []
        place = nil
        pending = nil
        found = [:]
        revealing = nil
        activity = Activity()
        hasEarlier = false
        setLive(live)
        keepsRows = keepingRows
        guard !keepingRows else {
            hooks?.keep()
            return
        }
        showState()
    }

    /// The thread's rows aren't coming: the view lets go of the ones it kept.
    func dropKeptRows() {
        guard keepsRows else { return }
        keepsRows = false
        showState()
    }

    /// Tells `keep` where the view leaves the thread's rows, nil at the end. It isn't told while the
    /// view still shows the rows of the thread before.
    func leave(_ keep: (TranscriptPlace?) -> Void) {
        guard threadID != nil, !keepsRows else { return }
        keep(hooks.map { $0.place() } ?? place)
    }

    private func showState() {
        hooks?.reset(rows, nil)
        hooks?.activity(activity)
    }

    /// The message on screen now belongs to a thread; its rows are about to arrive.
    func adopt(threadID: String) {
        self.threadID = threadID
    }

    func apply(reset: Bool, start: Int, remove: Int, rows new: [RowModel], earlier: Bool, at: TranscriptPlace? = nil) {
        hasEarlier = earlier
        defer { hooks?.earlier(earlier) }
        // The server has the message now, so the copy shown while it travelled goes. The view lets
        // go of its own with these rows, so that the message doesn't move.
        let sent = pending != nil && new.contains(where: \.isSentMessage)
        if reset {
            if sent { pending = nil }
            rows = new
            place = at
            hooks?.reset(new, at)
            if let pending { hooks?.pending(pending) }
            guard keepsRows else { return }
            keepsRows = false
            hooks?.activity(activity)
            return
        }
        guard start >= 0, remove >= 0, start + remove <= rows.count else { return }
        if sent { pending = nil }
        rows.replaceSubrange(start..<(start + remove), with: new)
        hooks?.splice(start, remove, new)
    }

    func setPending(_ text: String?, attachments: [AttachedFile] = [], queued: Bool = false, skills: [SkillSpan] = [], collapsible: Bool = false) {
        pendingText = text
        pendingSkills = skills
        pendingCollapsible = collapsible
        pending = text.map { RowModel.pending(text: $0, attachments: attachments, queued: queued, skills: skills, collapsible: collapsible) }
        hooks?.pending(pending)
    }

    /// Shows the skills of the message on its way as chips, while it is still the one shown.
    func setPendingSkills(_ skills: [SkillSpan], of text: String, attachments: [AttachedFile], queued: Bool) {
        guard !skills.isEmpty, pending != nil, pendingText == text else { return }
        setPending(text, attachments: attachments, queued: queued, skills: skills, collapsible: pendingCollapsible)
    }

    /// Shows the message on its way cut, as it will be once the server has it.
    func setPendingCollapsible(of text: String, attachments: [AttachedFile]) {
        guard pending != nil, pendingText == text else { return }
        setPending(text, attachments: attachments, skills: pendingSkills, collapsible: true)
    }

    /// While the view keeps the rows of the thread open before, what this one's agent does waits
    /// to be shown with its rows.
    func setActivity(_ activity: Activity) {
        self.activity = activity
        guard !keepsRows else { return }
        hooks?.activity(activity)
    }

    func setLive(_ live: Bool) {
        self.live = live
        hooks?.live(live)
    }

    /// A program row changed: it is shown as `make` makes it from the row it was.
    func replace(rowID: String, with make: (RowModel) -> RowModel?) {
        guard let index = rows.lastIndex(where: { $0.id == rowID }), let row = make(rows[index]) else { return }
        rows[index] = row
        hooks?.splice(index, 1, [row])
    }

    /// Where finding starts in what the view shows, under a find bar that covers `cover`.
    func findFrom(cover: CGFloat) -> FindFrom? { hooks?.findFrom(cover) }

    func setFound(_ found: [String: FoundMarks]) {
        self.found = found
        hooks?.found(found)
    }

    func reveal(rowID: String, range: NSRange?) {
        guard let hooks else {
            revealing = (rowID, range)
            return
        }
        hooks.reveal(rowID, range)
    }

    func apply(spans: [NSNumber], rowID: String) {
        guard let row = rows.last(where: { $0.id == rowID }), case .code(let content) = row.kind else { return }
        content.apply(spans: spans)
        hooks?.recolor(rowID, content)
    }
}
