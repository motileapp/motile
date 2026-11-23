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
    /// Where the whole image lies in the canvas's own points, when the canvas covers only the part
    /// of it on screen.
    var image: CGRect? {
        didSet { if image != oldValue { repaint() } }
    }
    private var imageRect: CGRect { image ?? bounds }
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
        let image = imageRect
        guard image.width > 0, image.height > 0 else { return point }
        return CGPoint(x: (point.x - image.minX) * session.width / image.width, y: (point.y - image.minY) * session.height / image.height)
    }

    /// A point in the image's pixels, in the canvas's own points.
    func point(_ pixel: CGPoint) -> CGPoint {
        let image = imageRect
        return CGPoint(x: image.minX + pixel.x * image.width / session.width, y: image.minY + pixel.y * image.height / session.height)
    }

    /// A length in the image's pixels, in the canvas's own points.
    func points(_ pixels: CGFloat) -> CGFloat {
        pixels * imageRect.width / session.width
    }

    /// A length on the screen, in the image's pixels, with the canvas zoomed by `zoom`.
    func pixels(_ length: CGFloat, zoom: CGFloat) -> CGFloat {
        let image = imageRect
        guard image.width > 0, zoom > 0 else { return length }
        return length / zoom * session.width / image.width
    }

    override func draw(_ rect: CGRect) {
        #if os(macOS)
        let context = NSGraphicsContext.current?.cgContext
        #else
        let context = UIGraphicsGetCurrentContext()
        #endif
        guard let scene, scene.width > 0, scene.height > 0, let context else { return }
        let image = imageRect
        context.translateBy(x: image.minX, y: image.minY)
        context.scaleBy(x: image.width / scene.width, y: image.height / scene.height)
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
