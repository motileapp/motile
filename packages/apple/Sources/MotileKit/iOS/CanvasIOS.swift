#if os(iOS)
import UIKit

extension CanvasView {
    /// A tap shows the tapped hit's tooltip, or hides it away from every hit; a sideways drag
    /// moves it along, and an upright one stays the transcript's scroll.
    func takePointer() {
        onPress = { [weak self] point in
            guard let self else { return }
            light(hit(at: point))
        }
        onDrag = { [weak self] _, point in
            guard let self, let index = hit(at: point) else { return }
            light(index)
        }
    }

    func watchAppearance() {
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in view.redraw() }
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
