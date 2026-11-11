#if os(macOS)
import AppKit
import SwiftUI

/// Keeps a mouse wheel's tick over the scroll view this lies in, pushing past its end, out of the
/// scroll view around it. A trackpad's gesture still goes on to it, as AppKit hands it over.
struct WheelHold: NSViewRepresentable {
    func makeNSView(context: Context) -> WheelHoldView { WheelHoldView() }
    func updateNSView(_ view: WheelHoldView, context: Context) {}
}

final class WheelHoldView: NSView {
    private var monitor: Any?

    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        guard window != nil else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
            guard let self, self.holds(event) else { return event }
            return nil
        }
    }

    /// A mouse wheel's tick has no phase; a trackpad's gesture and its momentum do.
    private func holds(_ event: NSEvent) -> Bool {
        guard event.phase.isEmpty, event.momentumPhase.isEmpty else { return false }
        return isOver(event) && pushesPastEnd(event)
    }

    private func isOver(_ event: NSEvent) -> Bool {
        guard event.window === window, let scroll = enclosingScrollView,
              let frame = window?.contentView?.superview,
              let hit = frame.hitTest(event.locationInWindow)
        else { return false }
        return hit.isDescendant(of: scroll)
    }

    private func pushesPastEnd(_ event: NSEvent) -> Bool {
        guard let scroll = enclosingScrollView, let document = scroll.documentView,
              abs(event.scrollingDeltaY) > abs(event.scrollingDeltaX)
        else { return false }
        let visible = scroll.documentVisibleRect
        let bounds = document.bounds
        let atTop = document.isFlipped ? visible.minY <= bounds.minY + 1 : visible.maxY >= bounds.maxY - 1
        let atBottom = document.isFlipped ? visible.maxY >= bounds.maxY - 1 : visible.minY <= bounds.minY + 1
        return event.scrollingDeltaY > 0 ? atTop : atBottom
    }
}
#endif
