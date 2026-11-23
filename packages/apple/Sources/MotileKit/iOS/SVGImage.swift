#if os(iOS)
import UIKit

/// Draws an SVG file, which iOS has nothing of its own to read, with the core.
enum SVGImage {
    nonisolated static func isSVG(_ data: Data) -> Bool {
        guard let start = String(data: data.prefix(600), encoding: .utf8) else { return false }
        return start.contains("<svg")
    }

    /// The SVG drawn `side` pixels on its longer side, off the main thread.
    static func image(of data: Data, side: CGFloat = 256) async -> UIImage? {
        await Task.detached(priority: .userInitiated) { Pictures.drawSVG(data, side: side).map { UIImage(cgImage: $0) } }.value
    }
}

final class SVGSize: NSObject, XMLParserDelegate {
    private var size: CGSize?

    /// The size the SVG asks to be drawn at, from its root's `width` and `height` or its `viewBox`.
    static func size(of data: Data) -> CGSize? {
        let reader = SVGSize()
        let parser = XMLParser(data: data)
        parser.delegate = reader
        parser.parse()
        return reader.size
    }

    func parser(_ parser: XMLParser, didStartElement name: String, namespaceURI: String?, qualifiedName: String?, attributes: [String: String] = [:]) {
        defer { parser.abortParsing() }
        let length = { (key: String) -> CGFloat? in
            guard let text = attributes[key], !text.hasSuffix("%") else { return nil }
            return Double(text.trimmingCharacters(in: .letters)).map { CGFloat($0) }
        }
        let box = attributes["viewBox"].map { $0.split(whereSeparator: { $0 == " " || $0 == "," }).compactMap { Double($0) } } ?? []
        let (width, height) = if let width = length("width"), let height = length("height") {
            (width, height)
        } else if box.count == 4 {
            (CGFloat(box[2]), CGFloat(box[3]))
        } else {
            (CGFloat(0), CGFloat(0))
        }
        guard width > 0, height > 0 else { return }
        size = CGSize(width: width, height: height)
    }
}
#endif
