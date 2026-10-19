import CoreGraphics
import CoreText
import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The colours a scene is painted in: its tokens in one appearance, on the surface it lies on,
/// with `current` the colour of the text around it.
final class CanvasPalette {
    let surface: Surface
    let dark: Bool
    let current: PlatformColor
    private var resolved: [CanvasColor: CGColor] = [:]

    init(surface: Surface = .background, dark: Bool, current: PlatformColor = Theme.foreground) {
        self.surface = surface
        self.dark = dark
        self.current = current
    }

    /// The colour, its alpha multiplied by `opacity`.
    func color(_ color: CanvasColor, opacity: CGFloat = 1) -> CGColor {
        let base = resolved[color] ?? resolve(color)
        resolved[color] = base
        return base.copy(alpha: base.alpha * min(max(opacity, 0), 1)) ?? base
    }

    private func resolve(_ color: CanvasColor) -> CGColor {
        Theme.resolved(color.platform(on: surface, current: current), dark: dark).cgColor
    }
}

/// Paints scenes with Core Graphics, as the core's reference painter does, into a context whose
/// origin is the scene's top left corner and whose y grows down, in points.
enum CanvasPainter {
    private static let sRGB = CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB()

    /// The scene's items in order, with the hit `lit` lit: its items just before `items[at]`.
    static func paint(_ scene: CanvasScene, in context: CGContext, palette: CanvasPalette, lit: Int? = nil) {
        context.saveGState()
        defer { context.restoreGState() }
        context.setShouldAntialias(true)
        context.setAllowsAntialiasing(true)
        context.setShouldSmoothFonts(false)
        let hit = lit.flatMap { scene.hits.indices.contains($0) ? scene.hits[$0] : nil }
        for (index, item) in scene.items.enumerated() {
            if let hit, hit.at == index { hit.lit.forEach { draw($0, in: context, palette: palette) } }
            draw(item, in: context, palette: palette)
        }
        guard let hit, hit.at >= scene.items.count else { return }
        hit.lit.forEach { draw($0, in: context, palette: palette) }
    }

    private static func draw(_ item: CanvasItem, in context: CGContext, palette: CanvasPalette) {
        switch item {
        case .shape(let path, let paint): shape(path, paint: paint, in: context, palette: palette)
        case .text(let text): self.text(text, in: context, palette: palette)
        }
    }

    private static func shape(_ path: CGPath, paint: CanvasPaint, in context: CGContext, palette: CanvasPalette) {
        if let gradient = paint.gradient {
            fill(path, with: gradient, opacity: paint.opacity, in: context, palette: palette)
        } else if let fill = paint.fill {
            context.setFillColor(palette.color(fill, opacity: paint.fillOpacity * paint.opacity))
            context.addPath(path)
            context.fillPath(using: .winding)
        }
        guard let stroke = paint.stroke else { return }
        context.setStrokeColor(palette.color(stroke.color, opacity: paint.opacity))
        context.setLineWidth(stroke.width)
        context.setLineCap(stroke.cap)
        context.setLineJoin(stroke.join)
        context.setMiterLimit(4)
        context.setLineDash(phase: 0, lengths: stroke.dash)
        context.addPath(path)
        context.strokePath()
        context.setLineDash(phase: 0, lengths: [])
    }

    private static func fill(_ path: CGPath, with gradient: CanvasGradient, opacity: CGFloat, in context: CGContext, palette: CanvasPalette) {
        let colors = gradient.stops.map { palette.color($0.color, opacity: $0.opacity * opacity) }
        let locations = gradient.stops.map(\.at)
        guard let shading = CGGradient(colorsSpace: sRGB, colors: colors as CFArray, locations: locations) else { return }
        context.saveGState()
        defer { context.restoreGState() }
        context.addPath(path)
        context.clip(using: .winding)
        context.drawLinearGradient(shading, start: gradient.start, end: gradient.end, options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
    }

    private static func text(_ text: CanvasText, in context: CGContext, palette: CanvasPalette) {
        context.saveGState()
        defer { context.restoreGState() }
        context.setFillColor(palette.color(text.color, opacity: text.opacity))
        context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
        context.textPosition = text.origin
        CTLineDraw(text.line, context)
    }
}
