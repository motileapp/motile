import Observation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The marks of an image being marked up, painted over the picture from the session's scene and
/// stretched to the picture's size, so they zoom with it. The zooming view passes it the pointer.
final class MarkupCanvas: PlatformView {
    let session: MarkupSession
    /// Told after the marks or the selected pin change.
    var changed: () -> Void = {}
    private var scene: CanvasScene? {
        didSet { repaint() }
    }
    private let palette = CanvasPalette(dark: true)

    init(session: MarkupSession) {
        self.session = session
        super.init(frame: .zero)
        #if os(iOS)
        isOpaque = false
        backgroundColor = .clear
        isUserInteractionEnabled = false
        contentMode = .redraw
        #endif
        watch()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError() }

    #if os(macOS)
    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    #endif

    /// A point in the canvas's own points, in the image's pixels.
    func pixel(_ point: CGPoint) -> CGPoint {
        guard bounds.width > 0, bounds.height > 0 else { return point }
        return CGPoint(x: point.x * session.width / bounds.width, y: point.y * session.height / bounds.height)
    }

    /// A point in the image's pixels, in the canvas's own points.
    func point(_ pixel: CGPoint) -> CGPoint {
        CGPoint(x: pixel.x * bounds.width / session.width, y: pixel.y * bounds.height / session.height)
    }

    /// A length on the screen, in the image's pixels, with the canvas zoomed by `zoom`.
    func pixels(_ length: CGFloat, zoom: CGFloat) -> CGFloat {
        guard bounds.width > 0, zoom > 0 else { return length }
        return length / zoom * session.width / bounds.width
    }

    override func draw(_ rect: CGRect) {
        #if os(macOS)
        let context = NSGraphicsContext.current?.cgContext
        #else
        let context = UIGraphicsGetCurrentContext()
        #endif
        guard let scene, scene.width > 0, scene.height > 0, let context else { return }
        context.scaleBy(x: bounds.width / scene.width, y: bounds.height / scene.height)
        CanvasPainter.paint(scene, in: context, palette: palette)
    }

    private func repaint() {
        #if os(macOS)
        needsDisplay = true
        #else
        setNeedsDisplay()
        #endif
    }

    private func watch() {
        withObservationTracking {
            scene = session.scene
            _ = session.selected
            changed()
        } onChange: { [weak self] in
            Task { @MainActor in self?.watch() }
        }
    }
}
