#if os(macOS)
import AppKit
import SwiftUI

/// Where a line that resizes is grabbed. The same view takes the drag and sets the cursor, so the
/// resize cursor shows wherever a drag would resize, whatever lies under it.
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

final class ResizeGrabView: NSView {
    var axis = Axis.horizontal {
        didSet {
            guard axis != oldValue else { return }
            window?.invalidateCursorRects(for: self)
        }
    }
    var dragged: (CGFloat?) -> Void = { _ in }
    private var start: CGPoint?
    private var tracking: NSTrackingArea?
    private var holding: CADisplayLink?

    private var cursor: NSCursor {
        axis == .horizontal ? .resizeLeftRight : .resizeUpDown
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        guard tracking == nil else { return }
        let options: NSTrackingArea.Options = [.cursorUpdate, .mouseEnteredAndExited, .mouseMoved, .activeInActiveApp, .inVisibleRect]
        let area = NSTrackingArea(rect: .zero, options: options, owner: self)
        addTrackingArea(area)
        tracking = area
    }

    override func resetCursorRects() {
        guard !DropdownBackdrop.isShown(in: window) else { return }
        addCursorRect(bounds, cursor: cursor)
    }

    override func cursorUpdate(with event: NSEvent) {
        show(event)
    }

    override func mouseEntered(with event: NSEvent) {
        show(event)
        holding?.invalidate()
        holding = displayLink(target: self, selector: #selector(hold))
        holding?.add(to: .main, forMode: .common)
    }

    override func mouseMoved(with event: NSEvent) {
        show(event)
    }

    override func mouseExited(with event: NSEvent) {
        holding?.invalidate()
        holding = nil
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        guard newWindow == nil else { return }
        holding?.invalidate()
        holding = nil
    }

    private func show(_ event: NSEvent) {
        guard isUnderPointer(event) else { return }
        cursor.set()
    }

    /// AppKit sets the arrow when the pointer leaves the cursor area of a view under the grab, like
    /// a row's, and gives the cursor back only to a view the pointer enters. So while the pointer
    /// is on the grab, it takes its cursor back every frame something else took it.
    @objc private func hold() {
        guard NSCursor.current != cursor, NSApp.isActive, let window,
            !DropdownBackdrop.isShown(in: window),
            window.contentView?.hitTest(window.mouseLocationOutsideOfEventStream) === self
        else { return }
        cursor.set()
    }

    override func mouseDown(with event: NSEvent) {
        start = event.locationInWindow
        cursor.set()
    }

    override func mouseDragged(with event: NSEvent) {
        guard let start else { return }
        cursor.set()
        let location = event.locationInWindow
        dragged(axis == .horizontal ? location.x - start.x : start.y - location.y)
    }

    override func mouseUp(with event: NSEvent) {
        guard start != nil else { return }
        start = nil
        dragged(nil)
        guard isUnderPointer(event) else { return NSCursor.arrow.set() }
        cursor.set()
    }
}
#endif
