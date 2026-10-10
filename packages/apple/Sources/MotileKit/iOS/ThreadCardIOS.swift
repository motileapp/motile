#if os(iOS)
import SwiftUI
import UIKit

/// A layer over the whole window that lets touches through, so that a thread's card lies over
/// the thread beside the sidebar.
final class ThreadCardWindow {
    private let host = UIHostingController(rootView: AnyView(EmptyView()))

    var isShown: Bool { host.view.superview != nil }

    init() {
        host.view.backgroundColor = .clear
        host.view.isUserInteractionEnabled = false
        host.safeAreaRegions = []
    }

    /// Shows the card beside the row, `row` in `view`.
    func show(_ card: AnyView, beside row: CGRect, in view: UIView, fading: Bool) {
        guard let window = view.window else { return }
        let inWindow = view.convert(row, to: window)
        host.rootView = AnyView(ThreadCardLayer(card: card, rowTop: inWindow.minY))
        let x = ThreadCardLayer.left(besideRowEndingAt: inWindow.maxX)
        host.view.frame = CGRect(x: x, y: 0, width: ThreadCardLayer.width, height: window.bounds.height)
        guard host.view.superview !== window else { return }
        hide()
        host.view.alpha = fading ? 0 : 1
        window.addSubview(host.view)
        guard fading else { return }
        UIView.animate(withDuration: 0.1) { [host] in host.view.alpha = 1 }
    }

    func hide() {
        host.view.removeFromSuperview()
    }
}
#endif
