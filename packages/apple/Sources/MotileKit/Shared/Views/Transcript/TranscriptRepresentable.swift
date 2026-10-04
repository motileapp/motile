import SwiftUI

/// Puts the transcript in SwiftUI and joins it to one of the store's transcript models: the open
/// thread's, or that of an agent the thread started.
struct TranscriptRepresentable {
    let store: AppStore
    var ofAgent = false
    /// The height of what covers the top of the transcript.
    var topInset: CGFloat = 0
    /// The height of what floats over the end of the transcript.
    let bottomInset: CGFloat

    private var model: TranscriptModel { ofAgent ? store.agentTranscript : store.transcript }

    private func make(_ coordinator: Coordinator) -> TranscriptView {
        let view = TranscriptView()
        let (store, model) = (store, model)
        view.topInset = topInset
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
        coordinator.model = model
        return view
    }

    private func update(_ view: TranscriptView) {
        view.topInset = topInset
        view.bottomInset = bottomInset
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator {
        var model: TranscriptModel?
    }
}

#if os(macOS)
extension TranscriptRepresentable: NSViewRepresentable {
    func makeNSView(context: Context) -> TranscriptView { make(context.coordinator) }

    func updateNSView(_ view: TranscriptView, context: Context) { update(view) }

    static func dismantleNSView(_ view: TranscriptView, coordinator: Coordinator) {
        coordinator.model?.detach(owner: view)
    }
}
#else
extension TranscriptRepresentable: UIViewRepresentable {
    func makeUIView(context: Context) -> TranscriptView { make(context.coordinator) }

    func updateUIView(_ view: TranscriptView, context: Context) { update(view) }

    static func dismantleUIView(_ view: TranscriptView, coordinator: Coordinator) {
        coordinator.model?.detach(owner: view)
    }
}
#endif
