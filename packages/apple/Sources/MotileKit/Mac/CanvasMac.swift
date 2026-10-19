#if os(macOS)
import AppKit

extension CanvasView {
    /// The pointer lights the hit it is over and shows its tooltip; leaving clears it.
    func takePointer() {
        onHover = { [weak self] point in
            guard let self else { return }
            light(point.flatMap { self.hit(at: $0) })
        }
    }

    /// The Mac redraws a view when its appearance changes.
    func watchAppearance() {}
}

extension NSView {
    /// Whether the view is drawn in the dark.
    var drawsDark: Bool {
        effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
    }

    /// The view over the whole window that what plays over everything is put in.
    var overlayHost: NSView? { window?.contentView }

    /// An image to assistive technologies, read as `label`.
    func describeImage(_ label: String?) {
        setAccessibilityElement(label != nil)
        setAccessibilityRole(.image)
        setAccessibilityLabel(label)
    }
}
#endif
