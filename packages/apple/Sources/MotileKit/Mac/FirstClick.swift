#if os(macOS)
import AppKit
import SwiftUI

extension View {
    /// Does `action` on the click that brings the window forward too, which SwiftUI's buttons let go
    /// by. Only for what is harmless to do by accident.
    func takesFirstClick(_ action: @escaping () -> Void) -> some View {
        overlay(FirstClick(action: action))
    }
}

private struct FirstClick: NSViewRepresentable {
    let action: () -> Void

    func makeNSView(context: Context) -> FirstClickView {
        FirstClickView()
    }

    func updateNSView(_ view: FirstClickView, context: Context) {
        view.action = action
    }
}

/// Lies over the view and takes a click only while its window isn't key, so that the view under it
/// has every other.
private final class FirstClickView: NSView {
    var action: () -> Void = {}

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard window?.isKeyWindow == false, frame.contains(point) else { return nil }
        return self
    }

    override func mouseDown(with event: NSEvent) {}

    override func mouseUp(with event: NSEvent) {
        guard bounds.contains(convert(event.locationInWindow, from: nil)) else { return }
        action()
    }
}
#endif
