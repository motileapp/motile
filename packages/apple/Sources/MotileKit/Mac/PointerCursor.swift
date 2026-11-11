#if os(macOS)
import AppKit
import ObjectiveC
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
/// top under it says the cursor, or the view a drag started on while it drags. Wherever it says
/// one, any other cursor set is dropped before it shows, like the I-beam AppKit's text sets
/// under a dropdown, and the cursor is said again. Views never set the cursor themselves.
enum PointerCursor {
    private static let events: Set<NSEvent.EventType> = [
        .mouseMoved, .mouseEntered, .mouseExited, .cursorUpdate, .scrollWheel,
        .leftMouseDown, .leftMouseDragged, .leftMouseUp, .rightMouseDown, .rightMouseUp,
    ]
    /// How far in from its edges a window is resized, under the system's cursor.
    private static let resizeEdge: CGFloat = 4
    private static weak var dragged: NSView?
    private static var setting = false
    private static var refreshing = false
    private static var found: (location: CGPoint, windows: [Int], number: Int)?

    /// Has every cursor set go through `allows` first.
    static func install() {
        guard let set = class_getInstanceMethod(NSCursor.self, #selector(NSCursor.set)),
            let gated = class_getInstanceMethod(NSCursor.self, #selector(NSCursor.gatedSet))
        else { return }
        method_exchangeImplementations(set, gated)
        let center = NotificationCenter.default
        for name in [NSApplication.didBecomeActiveNotification, NSWindow.didBecomeKeyNotification] {
            center.addObserver(forName: name, object: nil, queue: .main) { _ in
                found = nil
                update(again: true)
            }
        }
    }

    /// Whether a cursor set may show: the one set here, or any where nothing here says the cursor.
    fileprivate static func allows() -> Bool {
        guard !setting, Thread.isMainThread, NSApp.isActive, let hit = hit(),
            cursor(of: hit.view, at: hit.location) != nil
        else { return true }
        refresh()
        return false
    }

    static func update(after event: NSEvent) {
        found = nil
        guard events.contains(event.type) else { return }
        if event.type == .leftMouseDown { dragged = hit()?.view }
        update()
    }

    /// Sets the cursor for what lies under the pointer now, as after a dropdown opens or closes.
    static func refresh() {
        guard !refreshing else { return }
        refreshing = true
        DispatchQueue.main.async {
            refreshing = false
            update()
        }
    }

    /// `again` sets the cursor even where it seems set, as when another app's may show.
    private static func update(again: Bool = false) {
        guard NSApp.isActive, let hit = hit() else { return }
        let dragging = NSEvent.pressedMouseButtons & 1 != 0
        let view = dragging ? dragged ?? hit.view : hit.view
        guard let cursor = cursor(of: view, at: hit.location), again || cursor != NSCursor.current else { return }
        setting = true
        defer { setting = false }
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
    /// Asking the window server waits on it, and AppKit sets the cursor many times a frame while
    /// views change, so the answer stands until an event comes, the pointer moves or our windows change.
    private static func windowUnderPointer() -> NSWindow? {
        let location = NSEvent.mouseLocation
        let windows = NSApp.windows.filter(\.isVisible).map(\.windowNumber)
        if let found, found.location == location, found.windows == windows {
            return NSApp.window(withWindowNumber: found.number)
        }
        var number = NSWindow.windowNumber(at: location, belowWindowWithWindowNumber: 0)
        while let window = NSApp.window(withWindowNumber: number), window.ignoresMouseEvents {
            number = NSWindow.windowNumber(at: location, belowWindowWithWindowNumber: number)
        }
        found = (location, windows, number)
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

extension NSCursor {
    /// Takes `set`'s place once `PointerCursor.install` swapped them, and calls AppKit's own `set`
    /// where `PointerCursor` allows it.
    @objc dynamic fileprivate func gatedSet() {
        guard PointerCursor.allows() else { return }
        gatedSet()
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
