import CoreGraphics
import CoreText
import Foundation

/// The pins a map shows, as its style's `pins` source takes them.
struct MapPins: Equatable {
    struct Pin: Equatable {
        let key: String
        let label: String
        let lat: Double
        let lng: Double
        let number: Int
        let selected: Bool
        /// One emoji, which is also its image's name, else the pin shows its number.
        let icon: String?
    }

    let pins: [Pin]

    /// The pins of those given that are in the category shown.
    init(_ pins: [UiMapPin]) {
        self.pins = pins.filter(\.visible).map { pin in
            Pin(key: pin.key, label: pin.label, lat: pin.lat, lng: pin.lng, number: pin.number, selected: pin.selected, icon: pin.icon)
        }
    }

    /// Every emoji the pins show, once each, in the order they come.
    var icons: [String] {
        var seen = Set<String>()
        return pins.compactMap(\.icon).filter { seen.insert($0).inserted }
    }

    /// A FeatureCollection of a Point for each pin, its `icon` only when it has an emoji.
    var geoJSON: [String: Any] {
        let features: [[String: Any]] = pins.map { pin in
            var properties: [String: Any] = ["key": pin.key, "label": pin.label, "number": String(pin.number), "selected": pin.selected]
            if let icon = pin.icon { properties["icon"] = icon }
            return ["type": "Feature", "geometry": ["type": "Point", "coordinates": [pin.lng, pin.lat]], "properties": properties]
        }
        return ["type": "FeatureCollection", "features": features]
    }

    /// The route's source: a LineString of the `[lat, lng]` points, or nothing under two of them.
    static func route(_ line: [[Double]]) -> [String: Any] {
        guard line.count > 1 else { return ["type": "FeatureCollection", "features": [[String: Any]]()] }
        let coordinates = line.map { [$0[1], $0[0]] }
        return ["type": "Feature", "properties": [String: Any](), "geometry": ["type": "LineString", "coordinates": coordinates]]
    }

    static func json(_ object: Any) -> String {
        guard let data = try? JSONSerialization.data(withJSONObject: object) else { return "null" }
        return String(data: data, encoding: .utf8) ?? "null"
    }
}

/// A pin's emoji as the map draws it: at 18 points in the middle of a clear 34-point square.
enum MapEmoji {
    static let side: CGFloat = 34
    static let size: CGFloat = 18

    /// The emoji at `scale` pixels a point.
    static func image(_ emoji: String, scale: CGFloat) -> CGImage? {
        drawn(emoji, scale: scale)?.makeImage()
    }

    /// The emoji's pixels, red, green, blue and alpha, not premultiplied, as MapLibre GL JS takes them.
    static func pixels(_ emoji: String, scale: CGFloat) -> (side: Int, rgba: Data)? {
        guard let context = drawn(emoji, scale: scale), let data = context.data else { return nil }
        let count = context.bytesPerRow * context.height
        var bytes = Data(bytes: data, count: count)
        bytes.withUnsafeMutableBytes { (pixels: UnsafeMutableRawBufferPointer) in
            for index in stride(from: 0, to: count, by: 4) {
                let alpha = Int(pixels[index + 3])
                guard alpha > 0, alpha < 255 else { continue }
                for channel in 0..<3 {
                    pixels[index + channel] = UInt8(min(255, Int(pixels[index + channel]) * 255 / alpha))
                }
            }
        }
        return (context.width, bytes)
    }

    private static func drawn(_ emoji: String, scale: CGFloat) -> CGContext? {
        let pixels = Int((side * scale).rounded())
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
            let context = CGContext(
                data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: pixels * 4, space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { return nil }
        context.scaleBy(x: scale, y: scale)
        let font = CTFontCreateUIFontForLanguage(.system, size, nil) ?? CTFontCreateWithName("AppleColorEmoji" as CFString, size, nil)
        let text = NSAttributedString(string: emoji, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font])
        let line = CTLineCreateWithAttributedString(text)
        let bounds = CTLineGetBoundsWithOptions(line, [])
        context.textPosition = CGPoint(x: (side - bounds.width) / 2 - bounds.minX, y: (side - bounds.height) / 2 - bounds.minY)
        CTLineDraw(line, context)
        return context
    }
}

/// The map's style and the files the clients bundle in `Map`: the generated styles of
/// `packages/theme` and, for the Mac, MapLibre GL JS and the page it runs in.
enum MapStyle {
    private static var styles: [Bool: [String: Any]] = [:]
    private static let lock = NSLock()

    static func resource(_ name: String, _ kind: String) -> URL? {
        Bundle.main.url(forResource: name, withExtension: kind, subdirectory: "Map")
            ?? Bundle.module.url(forResource: name, withExtension: kind, subdirectory: "Map")
    }

    static func url(dark: Bool) -> URL? {
        resource(dark ? "map-style-dark" : "map-style-light", "json")
    }

    /// The style, read once for each appearance.
    static func style(dark: Bool) -> [String: Any] {
        lock.lock()
        defer { lock.unlock() }
        if let style = styles[dark] { return style }
        let read = url(dark: dark).flatMap { try? Data(contentsOf: $0) }.flatMap { try? JSONSerialization.jsonObject(with: $0) as? [String: Any] }
        styles[dark] = read ?? [:]
        return read ?? [:]
    }

    /// The style with its `pins` and `route` sources holding these, for a picture of the map.
    static func filled(dark: Bool, pins: MapPins?, line: [[Double]]) -> Data? {
        var style = style(dark: dark)
        var sources = style["sources"] as? [String: Any] ?? [:]
        sources["pins"] = ["type": "geojson", "data": pins?.geoJSON ?? MapPins.route([])]
        sources["route"] = ["type": "geojson", "data": MapPins.route(line)]
        style["sources"] = sources
        return try? JSONSerialization.data(withJSONObject: style)
    }

    /// What the map's requests for tiles and glyphs say they come from.
    static let userAgent = "Motile/\(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev")"

    /// The style's attribution, each part with the page it links to.
    static let attribution: [(text: String, link: URL?)] = [
        ("OpenFreeMap", URL(string: "https://openfreemap.org")),
        (" © ", nil),
        ("OpenMapTiles", URL(string: "https://openmaptiles.org")),
        (" · Data from ", nil),
        ("OpenStreetMap", URL(string: "https://www.openstreetmap.org/copyright")),
    ]
}

/// The map's geometry (`crates/openui/LAYOUT.md`, Map).
enum MapLayout {
    /// How tall the map is in a window this wide, as the measuring pass asks it.
    static func height(_ map: UiKind.Map, windowWidth: CGFloat) -> CGFloat {
        min(CGFloat(map.height), max(280, 0.82 * windowWidth))
    }

    /// The column under which the pins' labels are smaller.
    static let narrowColumn: CGFloat = 640
    /// How far the expanded map is from the window's edges, and under which width it is nearer.
    static func expandedInset(windowWidth: CGFloat) -> CGFloat { windowWidth < 520 ? 12 : 24 }

    static let inset: CGFloat = 12
    static let gap: CGFloat = 8
    static let roundSide: CGFloat = 34
    static let roundSymbol: CGFloat = 18
    static let filterPadding = (leading: CGFloat(12), trailing: CGFloat(30))
    static let smallSymbol: CGFloat = 14
    static let expandHeight: CGFloat = 28
    static let expandPadding: CGFloat = 10
    static let zoomSide: CGFloat = 28
    static let noteTop: CGFloat = 56
    static let noteWidth: CGFloat = 275
    static let notePadding: CGFloat = 12
    static let errorBottom: CGFloat = 28
    static let overviewBottom: CGFloat = 22
    static let overviewPadding = (vertical: CGFloat(2), horizontal: CGFloat(7))
    static let attributionPadding = (vertical: CGFloat(1), horizontal: CGFloat(4))
    static let attributionLeft: CGFloat = 10

    static let tilesFailed = "Map tiles are temporarily unavailable. The places are still shown."
    static let routeOverview = "Route overview"
}
