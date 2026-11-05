#if os(macOS)
import AppKit
import SwiftUI

/// Where a line that resizes is grabbed, under the resize cursor.
struct ResizeGrab: NSViewRepresentable {
    let axis: Axis
    /// Called with how far the drag has gone, right or down, and with nothing when it ends.
    let dragged: (CGFloat?) -> Void

    func makeNSView(context: Context) -> ResizeGrabView {
        ResizeGrabView()
    }

    func updateNSView(_ view: ResizeGrabView, context: Context) {
        view.axis = axis
        view.dragged = dragged
    }
}

final class ResizeGrabView: NSView, CursorOwner {
    var axis = Axis.horizontal
    var dragged: (CGFloat?) -> Void = { _ in }
    private var start: CGPoint?

    func cursor(at point: CGPoint) -> NSCursor {
        axis == .horizontal ? .resizeLeftRight : .resizeUpDown
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        start = event.locationInWindow
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start else { return }
        let location = event.locationInWindow
        dragged(axis == .horizontal ? location.x - start.x : start.y - location.y)
    }

    override func mouseUp(with event: NSEvent) {
        guard start != nil else { return }
        start = nil
        dragged(nil)
    }
}
#endif
