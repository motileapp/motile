import SwiftUI

/// Puts the AppKit transcript in SwiftUI and joins it to one of the store's transcript models:
/// the open thread's, or that of an agent the thread started.
struct TranscriptRepresentable: NSViewRepresentable {
    let store: AppStore
    var ofAgent = false
    /// The height of what floats over the end of the transcript.
    let bottomInset: CGFloat

    private var model: TranscriptModel { ofAgent ? store.agentTranscript : store.transcript }

    func makeNSView(context: Context) -> TranscriptView {
        let view = TranscriptView()
        let (store, model) = (store, model)
        view.bottomInset = bottomInset
        view.measuresAhead = !ofAgent
        view.onNeedHighlight = { rowIDs in
            guard let threadID = model.threadID else { return }
            store.core.send("highlight", ["thread_id": threadID, "row_ids": rowIDs])
        }
        view.onToggleRow = { rowID in
            guard let threadID = model.threadID else { return }
            store.core.send("toggle_row", ["thread_id": threadID, "row_id": rowID])
        }
        view.onNeedMedia = { id, done in store.media(id, done: done) }
        view.onViewMedia = { media, index in store.view(media, at: index) }
        view.onSendQueued = { messageID in store.sendNow(queued: messageID) }
        view.onCancelQueued = { messageID in store.takeBack(queued: messageID) }
        view.onOpenAgent = { itemID in store.sidePanel.showAgent(itemID) }
        if !ofAgent {
            view.onNeedEarlier = { done in
                guard let threadID = model.threadID else { return done() }
                store.core.send("load_earlier", ["thread_id": threadID]) { _ in done() }
            }
            view.onTrimEarlier = { keepRows, done in
                guard let threadID = model.threadID else { return done() }
                store.core.send("trim_earlier", ["thread_id": threadID, "keep_rows": keepRows]) { _ in done() }
            }
            view.onOpenDiff = { itemID, path in store.sidePanel.showDiff(.turn(itemID), revealing: path) }
        }
        let hooks = TranscriptModel.Hooks(
            reset: { [weak view] rows in view?.reset(rows: rows) },
            splice: { [weak view] start, remove, rows in view?.splice(start: start, remove: remove, rows: rows) },
            pending: { [weak view] row in view?.setPending(row) },
            activity: { [weak view] activity in view?.setActivity(activity) },
            recolor: { [weak view] rowID, content in view?.recolor(rowID: rowID, content: content) },
            earlier: { [weak view] earlier in view?.setEarlier(earlier) }
        )
        model.attach(hooks, owner: view)
        context.coordinator.model = model
        return view
    }

    func updateNSView(_ view: TranscriptView, context: Context) {
        view.bottomInset = bottomInset
    }

    static func dismantleNSView(_ view: TranscriptView, coordinator: Coordinator) {
        coordinator.model?.detach(owner: view)
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator {
        var model: TranscriptModel?
    }
}
