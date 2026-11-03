#if os(macOS)
import AppKit
import SwiftUI

/// What an open dropdown lies on: the whole window around it, as a menu owns the pointer. A press
/// on it only closes the dropdown, and what lies under it is not lit, scrolled or given the cursor.
/// When the dropdown goes, what had the keys before it opened gets them back.
final class DropdownBackdrop: NSView {
    private static weak var shown: DropdownBackdrop?

    var close: () -> Void = {}
    private var tracking: NSTrackingArea?
    private weak var returns: NSResponder?

    /// Whether an open dropdown or what it lies on is over the view where the pointer is.
    static func covers(_ view: NSView, at event: NSEvent) -> Bool {
        isShown(in: view.window) && !view.isUnderPointer(event)
    }

    /// Whether a dropdown is open over the window, which then keeps the cursors of the views in it.
    static func isShown(in window: NSWindow?) -> Bool {
        guard let shown, !shown.isHiddenOrHasHiddenAncestor else { return false }
        return shown.window === window
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        arrive()
    }

    func arrive() {
        guard let window, Self.shown !== self else { return }
        Self.shown = self
        returns = Self.owner(of: window.firstResponder)
        window.resetCursorRects()
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        guard newWindow == nil else { return }
        leave()
    }

    /// Nothing outside the dropdown can take the keys while it is open, so whatever has them now,
    /// like its search field, gives them back: a field that went away with the keys would keep
    /// the next click on the window from typing. SwiftUI may keep the view in the window a while
    /// after the dropdown has gone, and says so here.
    func leave() {
        guard let window, Self.shown === self else { return }
        Self.shown = nil
        window.resetCursorRects()
        guard Self.owner(of: window.firstResponder) !== returns else { return }
        let back = (returns as? NSView).flatMap { $0.window === window ? $0 : nil }
        window.makeFirstResponder(back)
    }

    /// The view that has the keys: a field rather than the editor it types with.
    private static func owner(of responder: NSResponder?) -> NSResponder? {
        guard let editor = responder as? NSTextView, editor.isFieldEditor else { return responder }
        return editor.delegate as? NSResponder ?? editor
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        guard tracking == nil else { return }
        let area = NSTrackingArea(rect: .zero, options: [.cursorUpdate, .mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self)
        addTrackingArea(area)
        tracking = area
    }

    override func cursorUpdate(with event: NSEvent) {
        NSCursor.arrow.set()
    }

    override func mouseMoved(with event: NSEvent) {
        guard isUnderPointer(event) else { return }
        NSCursor.arrow.set()
    }

    override func mouseDown(with event: NSEvent) {
        close()
    }

    override func rightMouseDown(with event: NSEvent) {
        close()
    }

    override func otherMouseDown(with event: NSEvent) {
        close()
    }

    override func mouseDragged(with event: NSEvent) {}
    override func mouseUp(with event: NSEvent) {}
    override func rightMouseUp(with event: NSEvent) {}
    override func otherMouseUp(with event: NSEvent) {}
    override func scrollWheel(with event: NSEvent) {}
}

/// SwiftUI that AppKit hosts on its own, as a list's row: under an open dropdown the pointer
/// leaves it, so that nothing in it is lit.
final class UncoveredHostingView<Content: View>: NSHostingView<Content> {
    override func mouseEntered(with event: NSEvent) {
        guard !DropdownBackdrop.covers(self, at: event) else { return }
        super.mouseEntered(with: event)
    }

    override func mouseMoved(with event: NSEvent) {
        guard !DropdownBackdrop.covers(self, at: event) else { return super.mouseExited(with: event) }
        super.mouseMoved(with: event)
    }
}

/// `DropdownBackdrop` in SwiftUI.
struct DropdownBackdropView: NSViewRepresentable {
    let close: () -> Void

    func makeNSView(context: Context) -> DropdownBackdrop {
        DropdownBackdrop()
    }

    func updateNSView(_ backdrop: DropdownBackdrop, context: Context) {
        backdrop.close = close
        backdrop.arrive()
    }

    static func dismantleNSView(_ backdrop: DropdownBackdrop, coordinator: ()) {
        backdrop.leave()
    }
}
#endif
