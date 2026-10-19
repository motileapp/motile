import Foundation
import QuartzCore

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A CSS timing function, `cubic-bezier(x1, y1, x2, y2)`.
struct TimingCurve {
    let x1, y1, x2, y2: CGFloat

    init(_ points: [CGFloat]) {
        let points = points.count == 4 ? points : [0, 0, 1, 1]
        (x1, y1, x2, y2) = (points[0], points[1], points[2], points[3])
    }

    /// The progress at the moment `x`, from 0 to 1.
    func progress(_ x: CGFloat) -> CGFloat {
        guard x > 0 else { return 0 }
        guard x < 1 else { return 1 }
        var (low, high): (CGFloat, CGFloat) = (0, 1)
        for _ in 0..<40 {
            let middle = (low + high) / 2
            if Self.bezier(x1, x2, middle) < x { low = middle } else { high = middle }
        }
        return Self.bezier(y1, y2, (low + high) / 2)
    }

    private static func bezier(_ a: CGFloat, _ b: CGFloat, _ t: CGFloat) -> CGFloat {
        let u = 1 - t
        return 3 * u * u * t * a + 3 * u * t * t * b + t * t * t
    }
}

extension CanvasFlip {
    /// How wide the face is `seconds` into the flip, as a share of its width, and whether the back
    /// shows by then.
    func scale(at seconds: CGFloat) -> (scaleX: CGFloat, pastSwap: Bool) {
        let t = duration > 0 ? min(max(seconds / duration, 0), 1) : 1
        let pairs = frames.filter { $0.count == 2 }
        guard let first = pairs.first, let last = pairs.last else { return (1, t >= swapAt) }
        guard t > first[0] else { return (first[1], t >= swapAt) }
        guard let after = pairs.firstIndex(where: { $0[0] >= t }), after > 0 else { return (last[1], t >= swapAt) }
        let (a, b) = (pairs[after - 1], pairs[after])
        let span = b[0] - a[0]
        let share = span > 0 ? (t - a[0]) / span : 1
        return (a[1] + (b[1] - a[1]) * share, t >= swapAt)
    }
}

extension CanvasConfetti {
    /// Where a piece is: its rectangle before it turns, how far it has turned and how opaque it is.
    struct Frame: Equatable {
        let rect: CGRect
        let angle: CGFloat
        let opacity: CGFloat
    }

    /// Every piece `seconds` after the burst, over a window of `size`.
    func frames(in size: CGSize, at seconds: CGFloat) -> [Frame] {
        let curve = TimingCurve(self.curve)
        return pieces.map { piece in
            let p = piece.duration > 0 ? min(max((seconds - piece.delay) / piece.duration, 0), 1) : 1
            let e = curve.progress(p)
            let origin = CGPoint(x: piece.x * size.width + piece.drift * e, y: top + fall * size.height * e)
            return Frame(
                rect: CGRect(origin: origin, size: CGSize(width: width, height: height)), angle: piece.turn * e * .pi / 180,
                opacity: 1 + (endOpacity - 1) * e)
        }
    }
}

/// Plays a flip on the screen's frame clock: `frame` is told the face's width as a share of its
/// own and whether the back shows yet, every frame until it ends. Under reduced motion the faces
/// swap at once.
final class FlipDriver: NSObject {
    private let flip: CanvasFlip
    private let frame: (CGFloat, Bool) -> Void
    private var link: CADisplayLink?
    private var started: CFTimeInterval?

    init(_ flip: CanvasFlip, frame: @escaping (_ scaleX: CGFloat, _ pastSwap: Bool) -> Void) {
        self.flip = flip
        self.frame = frame
    }

    /// Starts it on the clock of the screen `view` is on.
    func start(on view: PlatformView) {
        stop()
        guard !Platform.reducesMotion else { return frame(1, true) }
        started = nil
        let link = view.ticker(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    func stop() {
        link?.invalidate()
        link = nil
    }

    @objc private func tick(_ link: CADisplayLink) {
        let now = link.timestamp
        let started = self.started ?? now
        self.started = started
        let seconds = CGFloat(now - started)
        let (scale, pastSwap) = flip.scale(at: seconds)
        frame(scale, pastSwap)
        guard seconds >= flip.duration else { return }
        stop()
    }
}

/// Celebration's confetti over the whole window, above everything and taking no input, until
/// its pieces have fallen. Nothing is drawn under reduced motion.
final class ConfettiView: LayerView {
    private let confetti: CanvasConfetti
    private let pieces: [CALayer]
    private var link: CADisplayLink?
    private var started: CFTimeInterval?

    private init(_ confetti: CanvasConfetti) {
        self.confetti = confetti
        pieces = confetti.pieces.map { _ in CALayer() }
        super.init(frame: .zero)
        #if os(iOS)
        isUserInteractionEnabled = false
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    #endif

    /// Plays it over the window `view` is in.
    static func play(_ confetti: CanvasConfetti, from view: PlatformView) {
        guard !Platform.reducesMotion, let host = view.overlayHost else { return }
        let overlay = ConfettiView(confetti)
        overlay.frame = host.bounds
        overlay.clipsToBounds = true
        host.addSubview(overlay)
        overlay.place(at: 0)
        let link = overlay.ticker(target: overlay, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        overlay.link = link
    }

    override func paint(_ layer: CALayer) {
        for (piece, spec) in zip(pieces, confetti.pieces) {
            if piece.superlayer !== layer { layer.addSublayer(piece) }
            piece.backgroundColor = resolved(spec.color.platform(on: .background, current: Theme.foreground))
            piece.cornerRadius = confetti.radius
        }
    }

    @objc private func tick(_ link: CADisplayLink) {
        let now = link.timestamp
        let started = self.started ?? now
        self.started = started
        let seconds = CGFloat(now - started)
        guard seconds < confetti.duration, let host = superview else { return finish() }
        if frame != host.bounds { frame = host.bounds }
        place(at: seconds)
    }

    private func place(at seconds: CGFloat) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        for (piece, frame) in zip(pieces, confetti.frames(in: bounds.size, at: seconds)) {
            piece.bounds = CGRect(origin: .zero, size: frame.rect.size)
            piece.position = CGPoint(x: frame.rect.midX, y: frame.rect.midY)
            piece.transform = CATransform3DMakeRotation(frame.angle, 0, 0, 1)
            piece.opacity = Float(frame.opacity)
        }
        CATransaction.commit()
    }

    private func finish() {
        link?.invalidate()
        link = nil
        removeFromSuperview()
    }
}
