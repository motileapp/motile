import CMotileCore
import CoreGraphics
import CoreText
import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// What a canvas component draws, as the core lays it out (`canvas::Scene`): shapes and labels in
/// points from its top left corner, the parts that light under the pointer with their tooltips, a
/// chart's legend and what moves. Decoded off the main thread, with its paths and lines built.
struct CanvasScene {
    let width: CGFloat
    let height: CGFloat
    /// A formula's baseline from its top.
    let baseline: CGFloat?
    let label: String?
    let items: [CanvasItem]
    let hits: [CanvasHit]
    let legend: CanvasLegend?
    let motion: CanvasMotion?

    var size: CGSize { CGSize(width: width, height: height) }

    /// The hit that holds the point, in the scene's points: the last of those that do.
    func hit(at point: CGPoint) -> Int? {
        hits.lastIndex { $0.region.contains(point) }
    }
}

enum CanvasItem {
    case shape(path: CGPath, paint: CanvasPaint)
    case text(CanvasText)
}

struct CanvasPaint {
    var fill: CanvasColor?
    var gradient: CanvasGradient?
    var fillOpacity: CGFloat = 1
    var stroke: CanvasStroke?
    var opacity: CGFloat = 1
}

struct CanvasStroke {
    let color: CanvasColor
    let width: CGFloat
    let cap: CGLineCap
    let join: CGLineJoin
    let dash: [CGFloat]
}

struct CanvasGradient {
    let start: CGPoint
    let end: CGPoint
    let stops: [(at: CGFloat, color: CanvasColor, opacity: CGFloat)]
}

/// One line of text, its baseline starting at `origin` (its anchor already applied), laid out once
/// in the font of its role and drawn in the colour the context fills with.
struct CanvasText {
    let origin: CGPoint
    let line: CTLine
    let color: CanvasColor
    let opacity: CGFloat
}

/// The text styles of a scene: `tick` 11 regular with tabular digits, `caption` 12 regular, or
/// the UI typeface at a size. They are the scene's own sizes, which iOS doesn't enlarge.
enum CanvasFont {
    case tick, caption
    case ui(size: CGFloat, weight: PlatformFont.Weight)

    var font: PlatformFont {
        switch self {
        case .tick: PlatformFont.monospacedDigitSystemFont(ofSize: 11, weight: .regular)
        case .caption: PlatformFont.systemFont(ofSize: 12)
        case .ui(let size, let weight): PlatformFont.systemFont(ofSize: size, weight: weight)
        }
    }
}

/// A colour of a scene: the text's colour where it lies, a token by name, a token's wash, a layer
/// of the surface it lies on, or sRGB bytes.
enum CanvasColor: Hashable {
    case current
    case token(String)
    case wash(String)
    case surface(Layer)
    case rgba(UInt8, UInt8, UInt8, UInt8)

    enum Layer: String, Decodable {
        case base, border, row, control, box
    }

    /// The colour on `surface`, with `current` the text colour around it; it follows the appearance.
    func platform(on surface: Surface, current: PlatformColor) -> PlatformColor {
        switch self {
        case .current: current
        case .token(let name): Theme.named(name) ?? Theme.foreground
        case .wash(let name): Theme.thinned(Theme.named(name) ?? Theme.foreground, .colorTint)
        case .surface(.base): surface.platform
        case .surface(.border): surface.platformBorder
        case .surface(.row): surface.platform(.row)
        case .surface(.control): surface.platform(.control)
        case .surface(.box): surface.platform(.box)
        case .rgba(let r, let g, let b, let a):
            Theme.hex(UInt32(r) << 16 | UInt32(g) << 8 | UInt32(b), alpha: CGFloat(a) / 255)
        }
    }
}

struct CanvasHit {
    let region: Region
    /// The item `lit` is painted before; after them all when it is their count.
    let at: Int
    let lit: [CanvasItem]
    let tip: CanvasTip

    enum Region {
        case rect(CGRect)
        case circle(center: CGPoint, radius: CGFloat)
        /// Angles in radians clockwise from the right, `start` ≤ `end` ≤ `start` + 2π.
        case sector(center: CGPoint, inner: CGFloat, outer: CGFloat, start: CGFloat, end: CGFloat)

        func contains(_ point: CGPoint) -> Bool {
            switch self {
            case .rect(let rect):
                return point.x >= rect.minX && point.x <= rect.maxX && point.y >= rect.minY && point.y <= rect.maxY
            case .circle(let center, let radius):
                return hypot(point.x - center.x, point.y - center.y) <= radius
            case .sector(let center, let inner, let outer, let start, let end):
                let distance = hypot(point.x - center.x, point.y - center.y)
                guard distance >= inner, distance <= outer else { return false }
                let turn = 2 * CGFloat.pi
                var angle = atan2(point.y - center.y, point.x - center.x) - start
                angle -= (angle / turn).rounded(.down) * turn
                return start + angle <= end
            }
        }
    }
}

/// A tooltip centred on `x` with its top at `y`, in the scene's points.
struct CanvasTip {
    let x: CGFloat
    let y: CGFloat
    let title: String
    let rows: [Row]

    struct Row {
        let color: CanvasColor?
        let name: String
        let value: String
    }
}

struct CanvasLegend {
    /// A pie's legend lies beside its disc rather than under it.
    let beside: Bool
    let entries: [Entry]

    struct Entry {
        let color: CanvasColor
        let label: String
        /// A pie slice's share, "42.5%".
        let share: String?
    }

    var isPie: Bool { entries.contains { $0.share != nil } }
}

enum CanvasMotion {
    case confetti(CanvasConfetti)
    case flip(CanvasFlip)
}

/// Pieces that fall over the whole window, each from its top at `top` and its left at its `x`
/// share of the window's width, as `ConfettiMotion` moves them.
struct CanvasConfetti: Decodable {
    let duration: CGFloat
    let curve: [CGFloat]
    let width: CGFloat
    let height: CGFloat
    let radius: CGFloat
    let top: CGFloat
    let fall: CGFloat
    let endOpacity: CGFloat
    let pieces: [Piece]

    struct Piece: Decodable {
        let x: CGFloat
        let color: CanvasColor
        let delay: CGFloat
        let duration: CGFloat
        let drift: CGFloat
        let turn: CGFloat
    }
}

/// A card turning over as a horizontal squash: `frames` are [moment, scale] pairs from 0 to 1,
/// and the back shows from `swapAt` on.
struct CanvasFlip: Decodable {
    let duration: CGFloat
    let curve: [CGFloat]
    let swapAt: CGFloat
    let frames: [[CGFloat]]
}

/// Lays canvas components out through the core, which answers with their scenes.
enum CanvasLayout {
    private static let queue = DispatchQueue(label: "app.motile.canvas", qos: .userInitiated)

    /// The scene of a component of `kind` with its props, `width` wide beside text `textSize`
    /// tall, laid out on the calling thread: never the main one.
    static func scene(kind: String, props: JSON, width: CGFloat, textSize: CGFloat = 14) -> CanvasScene? {
        let input: JSON = ["kind": kind, "props": props, "width": width, "text_size": textSize]
        guard let data = try? JSONSerialization.data(withJSONObject: input), let json = String(data: data, encoding: .utf8) else { return nil }
        guard let answer = json.withCString({ motile_ui_layout($0) }) else { return nil }
        defer { motile_free_string(answer) }
        return decode(Data(String(cString: answer).utf8))
    }

    /// The same off the main thread, handed to `done` on the main thread.
    static func scene(kind: String, props: JSON, width: CGFloat, textSize: CGFloat = 14, done: @escaping (CanvasScene?) -> Void) {
        queue.async {
            let scene = scene(kind: kind, props: props, width: width, textSize: textSize)
            DispatchQueue.main.async { done(scene) }
        }
    }

    static func decode(_ data: Data) -> CanvasScene? {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try? decoder.decode(RawScene.self, from: data).scene
    }
}

// MARK: - The scene's JSON

extension CanvasColor: Decodable {
    private struct Keyed: Decodable {
        let wash: String?
        let surface: Layer?
    }

    init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer()
        if let name = try? value.decode(String.self) {
            self = name == "current" ? .current : .token(name)
        } else if let bytes = try? value.decode([UInt8].self), bytes.count == 4 {
            self = .rgba(bytes[0], bytes[1], bytes[2], bytes[3])
        } else if let keyed = try? value.decode(Keyed.self), let wash = keyed.wash {
            self = .wash(wash)
        } else if let keyed = try? value.decode(Keyed.self), let surface = keyed.surface {
            self = .surface(surface)
        } else {
            self = .current
        }
    }
}

private struct RawScene: Decodable {
    let width: CGFloat
    let height: CGFloat
    let baseline: CGFloat?
    let label: String?
    let items: [RawItem]
    let hits: [RawHit]?
    let legend: RawLegend?
    let motion: RawMotion?

    var scene: CanvasScene {
        CanvasScene(
            width: width, height: height, baseline: baseline, label: label, items: items.compactMap(\.item),
            hits: (hits ?? []).map(\.hit), legend: legend?.legend, motion: motion?.motion)
    }
}

private struct RawStroke: Decodable {
    let color: CanvasColor
    let width: CGFloat
    let cap: String?
    let join: String?
    let dash: [CGFloat]?

    var stroke: CanvasStroke {
        let cap: CGLineCap = self.cap == "round" ? .round : self.cap == "square" ? .square : .butt
        let join: CGLineJoin = self.join == "round" ? .round : self.join == "bevel" ? .bevel : .miter
        return CanvasStroke(color: color, width: width, cap: cap, join: join, dash: dash ?? [])
    }
}

private struct RawGradient: Decodable {
    let x1, y1, x2, y2: CGFloat
    let stops: [Stop]

    struct Stop: Decodable {
        let at: CGFloat
        let color: CanvasColor
        let opacity: CGFloat
    }

    var gradient: CanvasGradient {
        CanvasGradient(
            start: CGPoint(x: x1, y: y1), end: CGPoint(x: x2, y: y2), stops: stops.map { ($0.at, $0.color, $0.opacity) })
    }
}

/// An item as its JSON has it: one object for every kind, its paint flattened into it.
private struct RawItem: Decodable {
    let kind: String
    let verbs: String?
    let points: [CGFloat]?
    let x, y, width, height, rx, ry, cx, cy, r: CGFloat?
    let fill: CanvasColor?
    let gradient: RawGradient?
    let fillOpacity: CGFloat?
    let stroke: RawStroke?
    let opacity: CGFloat?
    let text: String?
    let anchor: String?
    let role: String?
    let size: CGFloat?
    let weight: String?
    let color: CanvasColor?

    var item: CanvasItem? {
        switch kind {
        case "path": return verbs.map { shape(Self.path(verbs: $0, points: points ?? [])) }
        case "rect":
            let rect = CGRect(x: x ?? 0, y: y ?? 0, width: width ?? 0, height: height ?? 0)
            let rx = min(self.rx ?? 0, rect.width / 2)
            let ry = min(self.ry ?? 0, rect.height / 2)
            guard rx > 0, ry > 0 else { return shape(CGPath(rect: rect, transform: nil)) }
            return shape(CGPath(roundedRect: rect, cornerWidth: rx, cornerHeight: ry, transform: nil))
        case "circle":
            let radius = r ?? 0
            let box = CGRect(x: (cx ?? 0) - radius, y: (cy ?? 0) - radius, width: radius * 2, height: radius * 2)
            return shape(CGPath(ellipseIn: box, transform: nil))
        case "text": return textItem.map(CanvasItem.text)
        default: return nil
        }
    }

    private func shape(_ path: CGPath) -> CanvasItem {
        let paint = CanvasPaint(fill: fill, gradient: gradient?.gradient, fillOpacity: fillOpacity ?? 1, stroke: stroke?.stroke, opacity: opacity ?? 1)
        return .shape(path: path, paint: paint)
    }

    private var font: CanvasFont {
        switch role {
        case "tick": return .tick
        case "caption": return .caption
        default:
            let weight: PlatformFont.Weight = weight == "medium" ? .medium : weight == "semibold" ? .semibold : .regular
            return .ui(size: size ?? 14, weight: weight)
        }
    }

    private var textItem: CanvasText? {
        guard let text, !text.isEmpty else { return nil }
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font.font, NSAttributedString.Key(kCTForegroundColorFromContextAttributeName as String): true,
        ]
        let line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: attributes))
        let advance = CGFloat(CTLineGetTypographicBounds(line, nil, nil, nil))
        let x = x ?? 0
        let start = anchor == "middle" ? x - advance / 2 : anchor == "end" ? x - advance : x
        return CanvasText(
            origin: CGPoint(x: start, y: y ?? 0), line: line, color: color ?? .current, opacity: opacity ?? 1)
    }

    static func path(verbs: String, points: [CGFloat]) -> CGPath {
        let path = CGMutablePath()
        var at = 0
        func next() -> CGPoint? {
            guard at + 1 < points.count else { return nil }
            defer { at += 2 }
            return CGPoint(x: points[at], y: points[at + 1])
        }
        for verb in verbs {
            switch verb {
            case "M":
                guard let point = next() else { return path }
                path.move(to: point)
            case "L":
                guard let point = next() else { return path }
                path.addLine(to: point)
            case "Q":
                guard let control = next(), let point = next() else { return path }
                path.addQuadCurve(to: point, control: control)
            case "C":
                guard let first = next(), let second = next(), let point = next() else { return path }
                path.addCurve(to: point, control1: first, control2: second)
            case "Z":
                path.closeSubpath()
            default:
                return path
            }
        }
        return path
    }
}

private struct RawHit: Decodable {
    let shape: String
    let x, y, width, height, cx, cy, r, inner, outer, start, end: CGFloat?
    let at: Int
    let lit: [RawItem]?
    let tip: RawTip

    var hit: CanvasHit {
        let center = CGPoint(x: cx ?? 0, y: cy ?? 0)
        let region: CanvasHit.Region =
            switch shape {
            case "circle": .circle(center: center, radius: r ?? 0)
            case "sector": .sector(center: center, inner: inner ?? 0, outer: outer ?? 0, start: start ?? 0, end: end ?? 0)
            default: .rect(CGRect(x: x ?? 0, y: y ?? 0, width: width ?? 0, height: height ?? 0))
            }
        return CanvasHit(region: region, at: at, lit: (lit ?? []).compactMap(\.item), tip: tip.tip)
    }
}

private struct RawTip: Decodable {
    let x: CGFloat
    let y: CGFloat
    let title: String
    let rows: [Row]?

    struct Row: Decodable {
        let color: CanvasColor?
        let name: String?
        let value: String
    }

    var tip: CanvasTip {
        CanvasTip(x: x, y: y, title: title, rows: (rows ?? []).map { CanvasTip.Row(color: $0.color, name: $0.name ?? "", value: $0.value) })
    }
}

private struct RawLegend: Decodable {
    let beside: Bool?
    let entries: [Entry]

    struct Entry: Decodable {
        let color: CanvasColor
        let label: String
        let share: String?
    }

    var legend: CanvasLegend {
        CanvasLegend(beside: beside ?? false, entries: entries.map { CanvasLegend.Entry(color: $0.color, label: $0.label, share: $0.share) })
    }
}

private struct RawMotion: Decodable {
    let kind: String
    let confetti: CanvasConfetti?
    let flip: CanvasFlip?

    init(from decoder: Decoder) throws {
        let keys = try decoder.container(keyedBy: Kind.self)
        kind = try keys.decode(String.self, forKey: .kind)
        confetti = kind == "confetti" ? try CanvasConfetti(from: decoder) : nil
        flip = kind == "flip" ? try CanvasFlip(from: decoder) : nil
    }

    private enum Kind: String, CodingKey { case kind }

    var motion: CanvasMotion? {
        if let confetti { return .confetti(confetti) }
        return flip.map(CanvasMotion.flip)
    }
}
