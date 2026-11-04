#if os(macOS)
import AppKit
import SwiftUI

/// Keeps a wheel gesture that starts over the scroll view this lies in out of the scroll view
/// around it. AppKit hands a gesture that starts at an end, pushing past it, to the one around.
struct WheelHold: NSViewRepresentable {
    func makeNSView(context: Context) -> WheelHoldView { WheelHoldView() }
    func updateNSView(_ view: WheelHoldView, context: Context) {}
}

final class WheelHoldView: NSView {
    private var monitor: Any?
    /// The gesture under way started over the scroll view, pushing past its end, and goes nowhere.
    private var holding = false

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

    /// A gesture starts with the fingers moving on the trackpad; a mouse wheel's every tick is one.
    private func holds(_ event: NSEvent) -> Bool {
        guard !event.phase.contains(.mayBegin) else { return false }
        let tick = event.phase.isEmpty && event.momentumPhase.isEmpty
        guard tick || event.phase.contains(.began) else { return holding }
        holding = isOver(event) && pushesPastEnd(event)
        return holding
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
