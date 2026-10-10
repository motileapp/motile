#if os(macOS)
import AppKit
import SwiftUI

/// A window over the sidebar's that lets the pointer through, so that a thread's card lies over
/// the thread beside the sidebar.
final class ThreadCardWindow {
    private let panel = NSPanel(contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
    private let host = NSHostingView(rootView: AnyView(EmptyView()))
    private var resigned: Any?

    var isShown: Bool { panel.parent != nil }

    init() {
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.isReleasedWhenClosed = false
        panel.animationBehavior = .none
        host.sizingOptions = []
        host.safeAreaRegions = []
        panel.contentView = host
    }

    /// Shows the card beside the row, `row` in `view`.
    func show(_ card: AnyView, beside row: CGRect, in view: NSView, fading: Bool) {
        guard let window = view.window else { return }
        let onScreen = window.convertToScreen(view.convert(row, to: nil))
        let frame = window.frame
        host.rootView = AnyView(ThreadCardLayer(card: card, rowTop: frame.maxY - onScreen.maxY))
        let x = ThreadCardLayer.left(besideRowEndingAt: onScreen.maxX)
        panel.setFrame(CGRect(x: x, y: frame.minY, width: ThreadCardLayer.width, height: frame.height), display: true)
        panel.appearance = window.effectiveAppearance
        guard panel.parent !== window else { return }
        hide()
        panel.alphaValue = fading ? 0 : 1
        window.addChildWindow(panel, ordered: .above)
        resigned = NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: window, queue: .main) { _ in
            ThreadPeek.shared.hide()
        }
        guard fading else { return }
        NSAnimationContext.runAnimationGroup { [panel] context in
            context.duration = 0.1
            panel.animator().alphaValue = 1
        }
    }

    func hide() {
        resigned.map(NotificationCenter.default.removeObserver)
        resigned = nil
        panel.parent?.removeChildWindow(panel)
        panel.orderOut(nil)
    }
}
#endif
