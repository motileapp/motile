#if os(iOS)
import UIKit
import UIKit.UIGestureRecognizerSubclass

extension CanvasView {
    /// A tap shows the tapped hit's tooltip, or hides it on the same hit, away from every hit or
    /// anywhere else in the window; a sideways drag moves it along, and an upright one stays the
    /// transcript's scroll.
    func takePointer() {
        onPress = { [weak self] point in
            guard let self else { return }
            let tapped = hit(at: point)
            light(tapped == lit ? nil : tapped)
        }
        onDrag = { [weak self] _, point in
            guard let self, let index = hit(at: point) else { return }
            light(index)
        }
    }

    func watchTapsElsewhere() {
        guard lit != nil, let window else { return }
        let watcher = window.gestureRecognizers?.lazy.compactMap { $0 as? TapElsewhere }.first ?? TapElsewhere(in: window)
        watcher.canvases.add(self)
    }

    func watchAppearance() {
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in view.redraw() }
    }
}

/// Sees the taps in its window and never recognizes, so that what its views do with them is left
/// as it is. A tap lets go of the lit canvases it misses.
private final class TapElsewhere: UIGestureRecognizer, UIGestureRecognizerDelegate {
    let canvases = NSHashTable<CanvasView>.weakObjects()
    private var start: CGPoint?

    init(in window: UIWindow) {
        super.init(target: nil, action: nil)
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
        requiresExclusiveTouchType = false
        delegate = self
        window.addGestureRecognizer(self)
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        start = touches.first?.location(in: view)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        guard let start, let point = touches.first?.location(in: view), hypot(point.x - start.x, point.y - start.y) > 10 else { return }
        state = .failed
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        defer { state = .failed }
        guard start != nil, let touch = touches.first else { return }
        for canvas in canvases.allObjects where !canvas.bounds.contains(touch.location(in: canvas)) {
            canvases.remove(canvas)
            canvas.light(nil)
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .failed
    }

    override func reset() {
        start = nil
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
        true
    }
}

extension UIView {
    /// Whether the view is drawn in the dark.
    var drawsDark: Bool {
        traitCollection.userInterfaceStyle == .dark
    }

    /// The view over the whole window that what plays over everything is put in.
    var overlayHost: UIView? { window }

    /// An image to assistive technologies, read as `label`.
    func describeImage(_ label: String?) {
        isAccessibilityElement = label != nil
        accessibilityTraits = .image
        accessibilityLabel = label
    }
}
#endif
