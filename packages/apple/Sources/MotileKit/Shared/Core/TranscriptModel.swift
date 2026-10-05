import Foundation

/// The rows of the open thread, kept in step with the core, and passed on to the view that draws
/// them. The view registers itself with `attach`; until it has, the rows wait here.
final class TranscriptModel {
    struct Hooks {
        let reset: ([RowModel]) -> Void
        let splice: (Int, Int, [RowModel]) -> Void
        let pending: (RowModel?) -> Void
        let activity: (Activity) -> Void
        let recolor: (String, CodeContent) -> Void
        /// Whether the thread has turns before the first row.
        let earlier: (Bool) -> Void
        /// Whether the thread has caught up with its server.
        let live: (Bool) -> Void
    }

    private(set) var threadID: String?
    private(set) var rows: [RowModel] = []
    private(set) var activity = Activity()
    private var hasEarlier = false
    private(set) var live = false
    /// The view still shows the rows of the thread that was open before.
    private var keepsRows = false
    private var pending: RowModel?
    private var hooks: Hooks?
    private var owner: ObjectIdentifier?

    var isEmpty: Bool { rows.isEmpty && pending == nil }

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
        hooks.reset(rows)
        hooks.pending(pending)
        hooks.activity(activity)
        hooks.earlier(hasEarlier)
    }

    /// Lets go of the view, unless another one has attached since.
    func detach(owner: AnyObject) {
        guard self.owner == ObjectIdentifier(owner) else { return }
        hooks = nil
        self.owner = nil
    }

    /// Starts showing another thread, or none. Its rows follow from the core. With `keepingRows`
    /// the view shows the rows it has until they do, so that it isn't empty in between.
    func begin(threadID: String?, live: Bool = false, keepingRows: Bool = false) {
        self.threadID = threadID
        rows = []
        pending = nil
        activity = Activity()
        hasEarlier = false
        setLive(live)
        keepsRows = keepingRows
        guard !keepingRows else { return }
        showState()
    }

    /// The thread's rows aren't coming: the view lets go of the ones it kept.
    func dropKeptRows() {
        guard keepsRows else { return }
        keepsRows = false
        showState()
    }

    private func showState() {
        hooks?.reset(rows)
        hooks?.activity(activity)
    }

    /// The message on screen now belongs to a thread; its rows are about to arrive.
    func adopt(threadID: String) {
        self.threadID = threadID
    }

    func apply(reset: Bool, start: Int, remove: Int, rows new: [RowModel], earlier: Bool) {
        hasEarlier = earlier
        defer { hooks?.earlier(earlier) }
        // The server has the message now, so the copy shown while it travelled goes. The view lets
        // go of its own with these rows, so that the message doesn't move.
        let sent = pending != nil && new.contains(where: \.isSentMessage)
        if reset {
            if sent { pending = nil }
            rows = new
            hooks?.reset(new)
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

    func setPending(_ text: String?, attachments: [AttachedFile] = [], queued: Bool = false) {
        pending = text.map { RowModel.pending(text: $0, attachments: attachments, queued: queued) }
        hooks?.pending(pending)
    }

    func setActivity(_ activity: Activity) {
        self.activity = activity
        hooks?.activity(activity)
    }

    func setLive(_ live: Bool) {
        self.live = live
        hooks?.live(live)
    }

    func apply(spans: [NSNumber], rowID: String) {
        guard let row = rows.last(where: { $0.id == rowID }), case .code(let content) = row.kind else { return }
        content.apply(spans: spans)
        hooks?.recolor(rowID, content)
    }
}
