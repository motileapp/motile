import Foundation
import ImageIO
import QuartzCore

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// SearchImage and Image: the photo on the surface's box, shimmering while it is looked for, an
/// image's symbol when there is none.
final class ProgramImageView: ProgramNodeView {
    private let picture = CALayer()
    private let shimmer = CAGradientLayer()
    private let missing = SymbolView()
    private var shown: String?
    private var failed: String?

    override init(frame: CGRect) {
        super.init(frame: frame)
        #if os(macOS)
        wantsLayer = true
        #endif
        missing.isHidden = true
        addSubview(missing)
        shimmer.startPoint = CGPoint(x: 0, y: 0.5)
        shimmer.endPoint = CGPoint(x: 1, y: 0.5)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private var base: CALayer? {
        #if os(macOS)
        layer
        #else
        layer
        #endif
    }

    override func configure(_ box: ProgramBox) {
        let (photos, src, fit, radius): (UiPhotos, String?, UiFit, UiRadius)
        switch box.node.kind {
        case .searchImage(let image): (photos, src, fit, radius) = (image.photos, nil, image.fit, image.radius)
        case .image(let image): (photos, src, fit, radius) = (image.photos, image.src, image.fit, image.radius)
        default: return
        }
        clip(radius: ProgramStyle.radius(radius, side: min(box.size.width, box.size.height)))
        guard let base else { return }
        if picture.superlayer !== base { base.insertSublayer(picture, at: 0) }
        base.backgroundColor = resolvedColor(box.surface.platform(.box))
        picture.contentsGravity = fit == .cover ? .resizeAspectFill : .resizeAspect
        picture.masksToBounds = true
        let source = photos.state == .found ? photos.media.first.map { "media:\($0)" } : photos.state == .idle ? src.map { "url:\($0)" } : nil
        let waiting = photos.state == .waiting || source != nil && shown != source
        showShimmer(waiting && !Platform.reducesMotion, on: box.surface)
        let none = photos.state == .missing || (source == nil && photos.state != .waiting)
        missing.isHidden = !none
        missing.image = .lucide(ProgramGlyphs.image, side: 26)
        missing.tint = Theme.mutedStrongerForeground
        if none {
            picture.contents = nil
            shown = nil
        }
        guard let source, source != shown, source != failed else { return }
        load(source)
    }

    private func resolvedColor(_ color: PlatformColor) -> CGColor {
        #if os(macOS)
        var resolved = color.cgColor
        effectiveAppearance.performAsCurrentDrawingAppearance { resolved = color.cgColor }
        return resolved
        #else
        return color.resolvedColor(with: traitCollection).cgColor
        #endif
    }

    private func load(_ source: String) {
        let done = { [weak self] (image: CGImage?) in
            guard let self else { return }
            guard let image else {
                failed = source
                guard source.hasPrefix("url:") else { return }
                host?.send(nodeID, ["type": "fallback"])
                return
            }
            show(image, source: source)
        }
        if source.hasPrefix("media:") {
            host?.media(String(source.dropFirst(6))) { url in ProgramImages.load(url, done: done) }
        } else if let url = URL(string: String(source.dropFirst(4))) {
            ProgramImages.fetch(url, done: done)
        }
    }

    private func show(_ image: CGImage, source: String) {
        shown = source
        picture.contents = image
        showShimmer(false, on: box?.surface ?? .background)
        guard !Platform.reducesMotion else { return }
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = 0
        fade.toValue = 1
        fade.duration = 0.3
        fade.timingFunction = CAMediaTimingFunction(name: .default)
        picture.add(fade, forKey: "fade")
    }

    private func showShimmer(_ on: Bool, on surface: Surface) {
        guard on, let base else {
            shimmer.removeFromSuperlayer()
            return
        }
        let band = resolvedColor(surface.platform(.control))
        let edge = band.copy(alpha: Opacity.shimmerBandEdge.fixed) ?? band
        let middle = band.copy(alpha: Opacity.shimmerBandMiddle.fixed) ?? band
        shimmer.colors = [edge.copy(alpha: 0) ?? edge, edge, middle, edge, edge.copy(alpha: 0) ?? edge]
        shimmer.locations = [0, 0.2, 0.5, 0.8, 1]
        if shimmer.superlayer !== base { base.addSublayer(shimmer) }
        guard shimmer.animation(forKey: "sweep") == nil else { return }
        let sweep = CABasicAnimation(keyPath: "transform.translation.x")
        sweep.fromValue = -bounds.width
        sweep.toValue = bounds.width
        sweep.duration = 1.4
        sweep.repeatCount = .infinity
        shimmer.add(sweep, forKey: "sweep")
    }

    override func layoutNow() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        picture.frame = bounds
        shimmer.frame = bounds
        CATransaction.commit()
        let side: CGFloat = 26
        missing.frame = CGRect(x: ((bounds.width - side) / 2).rounded(), y: ((bounds.height - side) / 2).rounded(), width: side, height: side)
    }
}

/// The photos programs show, read off the main thread and kept a while.
enum ProgramImages {
    private static let cache = NSCache<NSString, CGImage>()
    private static let queue = DispatchQueue(label: "app.motile.program-images", qos: .userInitiated, attributes: .concurrent)

    static func load(_ file: URL?, done: @escaping (CGImage?) -> Void) {
        guard let file else { return done(nil) }
        if let cached = cache.object(forKey: file.path as NSString) { return done(cached) }
        queue.async {
            let image = CGImageSourceCreateWithURL(file as CFURL, nil).flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) }
            if let image { cache.setObject(image, forKey: file.path as NSString) }
            DispatchQueue.main.async { done(image) }
        }
    }

    static func fetch(_ url: URL, done: @escaping (CGImage?) -> Void) {
        if let cached = cache.object(forKey: url.absoluteString as NSString) { return done(cached) }
        var request = URLRequest(url: url)
        request.setValue("Motile", forHTTPHeaderField: "User-Agent")
        URLSession.shared.dataTask(with: request) { data, response, _ in
            let ok = (response as? HTTPURLResponse).map { (200..<300).contains($0.statusCode) } ?? false
            let image = ok ? data.flatMap { CGImageSourceCreateWithData($0 as CFData, nil) }.flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) } : nil
            if let image { cache.setObject(image, forKey: url.absoluteString as NSString) }
            DispatchQueue.main.async { done(image) }
        }.resume()
    }
}

/// An Icon: the Lucide glyph the core names, in its colour.
final class ProgramIconView: ProgramNodeView {
    private let glyph = SymbolView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(glyph)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .icon(let icon) = box.node.kind else { return }
        glyph.isHidden = icon.icon == nil
        glyph.image = icon.icon.map { .lucide($0.glyph, side: CGFloat(icon.size)) }
        glyph.tint = ProgramStyle.color(icon.color, on: box.surface)
    }

    override func layoutNow() {
        glyph.frame = bounds
    }
}

/// A Table: a card of its header and rows, which moves sideways when its columns are wider.
final class ProgramTableView: ProgramNodeView {
    private let face = SurfaceView()
    private let clipView = SidewaysClipView()
    private let content = FlippedView()
    private let header = SurfaceView()
    private var lines: [SurfaceView] = []

    override init(frame: CGRect) {
        super.init(frame: frame)
        face.fill = Theme.card
        face.stroke = Theme.borderCard
        face.radius = Radius.lg
        header.fill = Theme.cardAccent
        addSubview(face)
        addSubview(clipView)
        clipView.addSubview(content)
        content.addSubview(header)
        clip(radius: Radius.lg)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var holder: PlatformView { content }
    override var textHolder: PlatformView { content }

    override func configure(_ box: ProgramBox) {
        guard let layout = box.extra as? ProgramTable.Layout else { return }
        let rows = layout.rows
        header.frame = CGRect(x: 0, y: 0, width: layout.contentWidth, height: rows.first?.height ?? 0)
        let edges = rows.dropLast().map { $0.y + $0.height }
        while lines.count < edges.count {
            let line = SurfaceView()
            line.fill = Theme.borderCard
            content.addSubview(line)
            lines.append(line)
        }
        for (index, line) in lines.enumerated() {
            line.isHidden = index >= edges.count
            guard index < edges.count else { continue }
            line.frame = CGRect(x: 0, y: edges[index] - 1, width: layout.contentWidth, height: 1)
        }
        clipView.frame = bounds
        clipView.setContent(content, size: CGSize(width: layout.contentWidth, height: box.size.height))
    }

    override func layoutNow() {
        face.frame = bounds
        clipView.frame = bounds
    }
}

/// What stands for a map until its own view comes: a card of its title and its places.
final class ProgramCardView: ProgramNodeView {
    private let face = SurfaceView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        face.radius = Radius.lg
        addSubview(face)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        face.stroke = box.surface.platformBorder
        face.fill = box.surface.platform(.box)
    }

    override func layoutNow() {
        face.frame = bounds
    }
}

/// A Chart: the core's scene with its legend and tooltips.
final class ProgramChartView: ProgramNodeView {
    private let canvas = CanvasView()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(canvas)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        canvas.surface = box.surface
        canvas.content = box.extra as? CanvasContent
    }

    override func layoutNow() {
        canvas.frame = bounds
    }
}

/// Svg and a block Math: the scene painted at its place, in a view larger than it so that strokes
/// past its edges still show.
final class ProgramSceneView: ProgramNodeView {
    private let paper = ScenePaper()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(paper)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        paper.scene = box.extra as? CanvasScene
        paper.surface = box.surface
        paper.frame = (box.rects["scene"] ?? CGRect(origin: .zero, size: box.size)).insetBy(dx: -ScenePaper.outset, dy: -ScenePaper.outset)
        describeImage(paper.scene?.label)
    }
}

final class ScenePaper: FlippedView {
    static let outset: CGFloat = 8
    var scene: CanvasScene? { didSet { redraw() } }
    var surface = Surface.background { didSet { redraw() } }

    override init(frame: CGRect) {
        super.init(frame: frame)
        #if os(iOS)
        isUserInteractionEnabled = false
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in view.redraw() }
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    #endif

    override func draw(_ rect: CGRect) {
        guard let scene, let context = Platform.drawing else { return }
        context.saveGState()
        defer { context.restoreGState() }
        context.translateBy(x: Self.outset, y: Self.outset)
        CanvasPainter.paint(scene, in: context, palette: CanvasPalette(surface: surface, dark: drawsDark, current: Theme.foreground))
    }
}

/// A Flashcard: its front, and its back once it is turned over, which a press does.
final class ProgramFlashcardView: ProgramNodeView {
    private let card = FlippedView()
    private let face = SurfaceView()
    private var shownBack: Bool?
    private var driver: FlipDriver?
    private var lit = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        #if os(macOS)
        card.wantsLayer = true
        #endif
        face.radius = Radius.lg
        addSubview(card)
        card.addSubview(face)
        focusable = true
        pointer = .hand
        onPress = { [weak self] _ in
            guard let self else { return }
            host?.send(nodeID, ["type": "flip"])
        }
        onHover = { [weak self] point in
            self?.lit = point != nil
            self?.paint()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var holder: PlatformView { card }
    override var textHolder: PlatformView { card }

    override func didShow(_ box: ProgramBox) {
        guard case .flashcard(let node) = box.node.kind else { return }
        defer { shownBack = node.flipped }
        guard let was = shownBack, was != node.flipped, let flip = (box.extra as? ProgramFlashcardParts)?.flip else {
            showSide(node.flipped)
            return
        }
        showSide(was)
        let driver = FlipDriver(flip) { [weak self] scale, pastSwap in
            self?.squash(scale)
            self?.showSide(pastSwap ? node.flipped : was)
        }
        self.driver = driver
        driver.start(on: self)
    }

    private func showSide(_ back: Bool) {
        guard let box, let parts = box.extra as? ProgramFlashcardParts else { return }
        for (index, child) in box.children.enumerated() {
            children[child.id]?.isHidden = (index < parts.front) == back
        }
        text("front")?.isHidden = back
        text("back")?.isHidden = !back
        text("hint")?.isHidden = back
        face.fill = back ? box.surface.platform(.box) : box.surface.platform
        paint()
    }

    private func paint() {
        guard let box else { return }
        face.stroke = lit ? Theme.borderAccent : box.surface.platformBorder
    }

    private func squash(_ scale: CGFloat) {
        let scale = max(0.001, scale)
        #if os(macOS)
        let middle = card.bounds.midX
        card.layer?.transform = CATransform3DConcat(
            CATransform3DConcat(CATransform3DMakeTranslation(-middle, 0, 0), CATransform3DMakeScale(scale, 1, 1)), CATransform3DMakeTranslation(middle, 0, 0))
        #else
        card.transform = CGAffineTransform(scaleX: scale, y: 1)
        #endif
    }

    override func layoutNow() {
        #if os(iOS)
        card.transform = .identity
        #endif
        card.frame = bounds
        face.frame = card.bounds
    }
}

/// What a flashcard's view needs besides its parts: how many of its children are the front's, and
/// its flip.
struct ProgramFlashcardParts {
    let front: Int
    let flip: CanvasFlip?
}

/// A Celebration draws nothing in its row; each new burst plays its confetti over the window.
final class ProgramCelebrationView: ProgramNodeView {
    /// The bursts already seen, by node, so that a row scrolled back into view doesn't play again.
    private static var seen: [String: Int] = [:]

    override func configure(_ box: ProgramBox) {
        guard case .celebration(let node) = box.node.kind else { return }
        let key = (host?.rowID ?? "") + "#" + box.id
        let before = Self.seen[key]
        Self.seen[key] = node.burst
        guard let before, node.burst > before, case .confetti(let confetti)? = box.extra as? CanvasMotion else { return }
        ConfettiView.play(confetti, from: self)
    }
}
