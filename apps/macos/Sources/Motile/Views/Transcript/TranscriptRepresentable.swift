import SwiftUI

/// Puts the AppKit transcript in SwiftUI and joins it to the store's transcript model.
struct TranscriptRepresentable: NSViewRepresentable {
    let store: AppStore
    /// The height of what floats over the end of the transcript.
    let bottomInset: CGFloat

    func makeNSView(context: Context) -> TranscriptView {
        let view = TranscriptView()
        let store = store
        view.bottomInset = bottomInset
        view.onNeedHighlight = { rowIDs in
            guard let threadID = store.transcript.threadID else { return }
            store.core.send("highlight", ["thread_id": threadID, "row_ids": rowIDs])
        }
        view.onToggleRow = { rowID in
            guard let threadID = store.transcript.threadID else { return }
            store.core.send("toggle_row", ["thread_id": threadID, "row_id": rowID])
        }
        view.onNeedMedia = { id, done in store.media(id, done: done) }
        let hooks = TranscriptModel.Hooks(
            reset: { [weak view] rows in view?.reset(rows: rows) },
            splice: { [weak view] start, remove, rows in view?.splice(start: start, remove: remove, rows: rows) },
            pending: { [weak view] row in view?.setPending(row) },
            activity: { [weak view] activity in view?.setActivity(activity) },
            recolor: { [weak view] rowID, content in view?.recolor(rowID: rowID, content: content) }
        )
        store.transcript.attach(hooks, owner: view)
        context.coordinator.store = store
        return view
    }

    func updateNSView(_ view: TranscriptView, context: Context) {
        view.bottomInset = bottomInset
    }

    static func dismantleNSView(_ view: TranscriptView, coordinator: Coordinator) {
        coordinator.store?.transcript.detach(owner: view)
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator {
        var store: AppStore?
    }
}
