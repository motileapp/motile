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
    }

    private(set) var threadID: String?
    private(set) var rows: [RowModel] = []
    private(set) var activity = Activity()
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
        hooks.reset(rows)
        hooks.pending(pending)
        hooks.activity(activity)
    }

    /// Lets go of the view, unless another one has attached since.
    func detach(owner: AnyObject) {
        guard self.owner == ObjectIdentifier(owner) else { return }
        hooks = nil
        self.owner = nil
    }

    /// Starts showing another thread, or none. Its rows follow from the core.
    func begin(threadID: String?) {
        self.threadID = threadID
        rows = []
        pending = nil
        activity = Activity()
        hooks?.reset([])
        hooks?.activity(activity)
    }

    /// The message on screen now belongs to a thread; its rows are about to arrive.
    func adopt(threadID: String) {
        self.threadID = threadID
    }

    func apply(reset: Bool, start: Int, remove: Int, rows new: [RowModel]) {
        // The server has the message now, so the copy shown while it travelled goes. The view lets
        // go of its own with these rows, so that the message doesn't move.
        let sent = pending != nil && new.contains(where: \.isSentMessage)
        if reset {
            if sent { pending = nil }
            rows = new
            hooks?.reset(new)
            if let pending { hooks?.pending(pending) }
            return
        }
        guard start >= 0, remove >= 0, start + remove <= rows.count else { return }
        if sent { pending = nil }
        rows.replaceSubrange(start..<(start + remove), with: new)
        hooks?.splice(start, remove, new)
    }

    func setPending(_ text: String?, attachments: [AttachedFile] = []) {
        pending = text.map { RowModel.pending(text: $0, attachments: attachments) }
        hooks?.pending(pending)
    }

    func setActivity(_ activity: Activity) {
        self.activity = activity
        hooks?.activity(activity)
    }

    func apply(spans: [NSNumber], rowID: String) {
        guard let row = rows.last(where: { $0.id == rowID }), case .code(let content) = row.kind else { return }
        content.apply(spans: spans)
        hooks?.recolor(rowID, content)
    }
}
