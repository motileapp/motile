import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A canvas scene as a view: Chart, Svg or Math, painted as the core laid it out, with a chart's
/// legend and the tooltip of the part under the pointer, or the finger on iOS. It is transparent:
/// it lies on `surface`, which colours `{"surface": …}` and its legend's dots, and `textColor` is
/// the colour `current` paints in.
final class CanvasView: FlippedView {
    var content: CanvasContent? {
        didSet {
            lit = nil
            show()
        }
    }
    var surface = Surface.background { didSet { show() } }
    var textColor = Theme.foreground { didSet { show() } }
    /// The hit that is lit, whose tooltip shows.
    private(set) var lit: Int?
    private let tipView = CanvasTipView()
    private var legendViews: [PlatformView] = []

    init(content: CanvasContent? = nil) {
        self.content = content
        super.init(frame: .zero)
        addSubview(tipView)
        tipView.isHidden = true
        watchAppearance()
        show()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// How large it is: `content.size`.
    var size: CGSize { content?.size ?? .zero }

    override func draw(_ rect: CGRect) {
        guard let content, let context = Platform.drawing else { return }
        context.saveGState()
        defer { context.restoreGState() }
        context.translateBy(x: content.origin.x, y: content.origin.y)
        let palette = CanvasPalette(surface: surface, dark: drawsDark, current: textColor)
        CanvasPainter.paint(content.scene, in: context, palette: palette, lit: lit)
    }

    /// Lights the hit and shows its tooltip, or nothing.
    func light(_ index: Int?) {
        guard index != lit else { return }
        lit = index
        redraw()
        showTip()
    }

    func hit(at point: CGPoint) -> Int? {
        content?.hit(at: point)
    }

    private func show() {
        showLegend()
        showTip()
        redraw()
        describeImage(content?.scene.label)
        guard content?.scene.hits.isEmpty == false else { return }
        takePointer()
    }

    private func showLegend() {
        legendViews.forEach { $0.removeFromSuperview() }
        legendViews = (content?.legend ?? []).flatMap { mark in
            [dot(mark.dot, mark.color)] + mark.texts.map(label)
        }
        for view in legendViews { addSubview(view) }
        addSubview(tipView)
    }

    private func showTip() {
        guard let content, let lit, content.tips.indices.contains(lit) else {
            tipView.isHidden = true
            return
        }
        let tip = content.tips[lit]
        tipView.show(tip, dots: tip.dots.map { dot($0.frame, $0.color) }, labels: tip.texts.map(label))
        tipView.frame = tip.frame.offsetBy(dx: content.origin.x, dy: content.origin.y)
        tipView.isHidden = false
    }

    private func dot(_ frame: CGRect, _ color: CanvasColor) -> PlatformView {
        let dot = SurfaceView()
        dot.radius = frame.height / 2
        dot.fill = color.platform(on: surface, current: textColor)
        dot.frame = frame
        return dot
    }

    private func label(_ text: NSAttributedString, _ frame: CGRect) -> PlatformView {
        let label = TextBlockView()
        label.text = text
        label.frame = frame
        return label
    }
}

/// A chart's tooltip: a popover's card of its title and rows, over the scene and taking no input.
final class CanvasTipView: FlippedView {
    private let face = SurfaceView()
    private var parts: [PlatformView] = []

    override init(frame: CGRect) {
        super.init(frame: frame)
        face.fill = Surface.popover.platform
        face.stroke = Surface.popover.platformBorder
        face.radius = Radius.lg
        face.drop = (.md, .shadow)
        addSubview(face)
        #if os(iOS)
        isUserInteractionEnabled = false
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    #endif

    func show(_ tip: CanvasContent.Tip, dots: [PlatformView], labels: [PlatformView]) {
        parts.forEach { $0.removeFromSuperview() }
        parts = dots + labels
        for part in parts { addSubview(part) }
        relayout()
    }

    override func layoutNow() {
        face.frame = bounds
    }
}
