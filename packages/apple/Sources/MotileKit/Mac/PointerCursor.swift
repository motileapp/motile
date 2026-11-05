#if os(macOS)
import AppKit
import SwiftUI
import WebKit

/// The Mac app's application, which has the cursor set after each event of the pointer.
final class MotileApplication: NSApplication {
    override func sendEvent(_ event: NSEvent) {
        super.sendEvent(event)
        PointerCursor.update(after: event)
    }
}

/// A view that says its cursor, at a point in its own coordinates.
protocol CursorOwner: NSView {
    func cursor(at point: CGPoint) -> NSCursor
}

/// The one place the cursor is set. After AppKit has handled an event of the pointer, the view on
/// top under it says the cursor, or the view a drag started on while it drags. What AppKit's text,
/// SwiftUI and the system set while the event went through is set over, so nothing shows its
/// cursor through what lies over it. Views never set the cursor themselves.
enum PointerCursor {
    private static let events: Set<NSEvent.EventType> = [
        .mouseMoved, .mouseEntered, .mouseExited, .cursorUpdate, .scrollWheel,
        .leftMouseDown, .leftMouseDragged, .leftMouseUp, .rightMouseDown, .rightMouseUp,
    ]
    /// How far in from its edges a window is resized, under the system's cursor.
    private static let resizeEdge: CGFloat = 4
    private static weak var dragged: NSView?

    static func update(after event: NSEvent) {
        guard events.contains(event.type) else { return }
        if event.type == .leftMouseDown { dragged = hit()?.view }
        update(dragging: event.type == .leftMouseDragged)
    }

    /// Sets the cursor for what lies under the pointer now, as after a dropdown opens or closes.
    static func refresh() {
        DispatchQueue.main.async { update() }
    }

    private static func update(dragging: Bool = false) {
        guard NSApp.isActive, let hit = hit() else { return }
        let view = dragging ? dragged ?? hit.view : hit.view
        guard let cursor = cursor(of: view, at: hit.location), cursor != NSCursor.current else { return }
        cursor.set()
    }

    /// The view on top under the pointer, in the window under it, and where the pointer is in it.
    private static func hit() -> (view: NSView, location: CGPoint)? {
        guard let window = windowUnderPointer(), let content = window.contentView else { return nil }
        window.disableCursorRects()
        let location = window.mouseLocationOutsideOfEventStream
        let inside = window.styleMask.contains(.resizable) ? content.frame.insetBy(dx: resizeEdge, dy: resizeEdge) : content.frame
        guard inside.contains(location), let view = content.hitTest(location) else { return nil }
        return (view, location)
    }

    /// The window the pointer's events go to: the one on top under it that takes them, if it is ours.
    private static func windowUnderPointer() -> NSWindow? {
        var number = NSWindow.windowNumber(at: NSEvent.mouseLocation, belowWindowWithWindowNumber: 0)
        while let window = NSApp.window(withWindowNumber: number), window.ignoresMouseEvents {
            number = NSWindow.windowNumber(at: NSEvent.mouseLocation, belowWindowWithWindowNumber: number)
        }
        return NSApp.window(withWindowNumber: number)
    }

    /// The cursor over `view`: its own, its text's, or the arrow. A web page sets its own.
    private static func cursor(of view: NSView, at location: CGPoint) -> NSCursor? {
        var current: NSView? = view
        while let view = current {
            switch view {
            case let owner as CursorOwner: return owner.cursor(at: owner.convert(location, from: nil))
            case is WKWebView: return nil
            case let text as NSTextView: return text.isEditable || text.isSelectable ? .iBeam : .arrow
            case let field as NSTextField: return field.isEnabled && (field.isEditable || field.isSelectable) ? .iBeam : .arrow
            case is NSControl: return .arrow
            default: current = view.superview
            }
        }
        return .arrow
    }
}

/// `TypingArea` on the Mac: a view of its own, so that the cursor of text shows only where it is
/// the view on top.
struct TypingAreaMac: NSViewRepresentable {
    let press: (() -> Void)?

    func makeNSView(context: Context) -> TypingAreaView {
        TypingAreaView()
    }

    func updateNSView(_ view: TypingAreaView, context: Context) {
        view.onPress = press.map { press in { _ in press() } }
    }
}

final class TypingAreaView: FlippedView {
    override func cursor(at point: CGPoint) -> NSCursor {
        .iBeam
    }
}
#endif
