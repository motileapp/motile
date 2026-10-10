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
    /// How far up from its bottom the transcript fades out.
    var bottomFade: CGFloat = 0

    private var model: TranscriptModel { ofAgent ? store.agentTranscript : store.transcript }

    private func make(_ coordinator: Coordinator) -> TranscriptView {
        let view = TranscriptView()
        let (store, model) = (store, model)
        view.topInset = topInset
        view.bottomInset = bottomInset
        view.bottomFade = bottomFade
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
        view.onFetchFile = { id, done in store.fetchMedia(id, done: done) }
        view.onCancelFetch = { id in store.cancelMedia(id) }
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
            view.onOpenDiff = { itemID, path in
                guard let path else { return store.sidePanel.showDiff(.turn(itemID)) }
                store.sidePanel.showChange(turn: itemID, path: path)
            }
        }
        let hooks = TranscriptModel.Hooks(
            reset: { [weak view] rows in view?.reset(rows: rows) },
            splice: { [weak view] start, remove, rows in view?.splice(start: start, remove: remove, rows: rows) },
            pending: { [weak view] row in view?.setPending(row) },
            activity: { [weak view] activity in view?.setActivity(activity) },
            recolor: { [weak view] rowID, content in view?.recolor(rowID: rowID, content: content) },
            earlier: { [weak view] earlier in view?.setEarlier(earlier) },
            live: { [weak view] live in view?.live = live }
        )
        model.attach(hooks, owner: view)
        coordinator.model = model
        return view
    }

    private func update(_ view: TranscriptView) {
        view.topInset = topInset
        view.bottomInset = bottomInset
        view.bottomFade = bottomFade
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    final class Coordinator {
        var model: TranscriptModel?
    }
}

extension View {
    /// Puts the open thread's transcript behind the view, which holds the composer, and under
    /// the safe area of `edges`, the keyboard too: it lifts the composer and leaves the
    /// transcript as tall as it was. The transcript leaves room for the composer and fades out
    /// from the middle of its box down to its own bottom, wherever the two are.
    func transcriptBehind(of store: AppStore, shown: Bool, under edges: Edge.Set) -> some View {
        backgroundPreferenceValue(ComposerPlace.self) { place in
            if shown {
                GeometryReader { safe in
                    GeometryReader { proxy in
                        TranscriptRepresentable(
                            store: store,
                            topInset: edges.contains(.top) ? safe.safeAreaInsets.top : 0,
                            bottomInset: place.room.map { proxy.size.height - proxy[$0].minY } ?? 0,
                            bottomFade: place.box.map { proxy.size.height - proxy[$0].midY } ?? 0
                        )
                    }
                    .ignoresSafeArea(.all, edges: edges)
                }
            }
        }
    }
}

#if os(macOS)
extension TranscriptRepresentable: NSViewRepresentable {
    func makeNSView(context: Context) -> TranscriptView { make(context.coordinator) }

    func updateNSView(_ view: TranscriptView, context: Context) {
        view.putAway(context.environment.putAway)
        update(view)
    }

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
