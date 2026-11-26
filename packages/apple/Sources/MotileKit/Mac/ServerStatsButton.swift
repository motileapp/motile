#if os(macOS)
import SwiftUI

/// A server's line in the sidebar, which opens what its machine does. The line may lie in a list
/// AppKit draws, so the dropdown hangs from the line's own view.
struct ServerStatsButton<Label: View>: View {
    @Environment(AppStore.self) private var store
    let server: Server
    @ViewBuilder let label: () -> Label
    @State private var anchor = ViewAnchor()
    @State private var open = false

    var body: some View {
        label()
            .background(ViewAnchorView(anchor: anchor))
            .button(.highlight(radius: 0, lit: open, opaque: true)) { toggle() }
    }

    private func toggle() {
        guard let view = anchor.view else { return }
        let card = ServerStatsCard(server: server).environment(store)
        HungDropdown.shared.toggle(from: view, content: AnyView(card)) { open = false }
        open = HungDropdown.shared.isOpen(from: view)
    }
}

final class ViewAnchor {
    weak var view: NSView?
}

private struct ViewAnchorView: NSViewRepresentable {
    let anchor: ViewAnchor

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        anchor.view = view
        return view
    }

    func updateNSView(_ view: NSView, context: Context) {
        anchor.view = view
    }
}
#endif
