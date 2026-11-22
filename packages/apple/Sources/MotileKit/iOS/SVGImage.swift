#if os(iOS)
import UIKit
import WebKit

/// Draws an SVG file, which iOS has nothing to read with but a web view: one that is never seen
/// draws it, and a picture is taken of that.
@MainActor
final class SVGImage: NSObject, WKNavigationDelegate {
    private static var drawing: [SVGImage] = []

    private let view: WKWebView
    private var done: ((UIImage?) -> Void)?

    nonisolated static func isSVG(_ data: Data) -> Bool {
        guard let start = String(data: data.prefix(600), encoding: .utf8) else { return false }
        return start.contains("<svg")
    }

    /// The SVG drawn `side` pixels on its longer side, in the shape it asks for.
    static func image(of data: Data, side: CGFloat = 256) async -> UIImage? {
        await withCheckedContinuation { continuation in
            let image = SVGImage()
            drawing.append(image)
            image.draw(data, side: side) { result in
                drawing.removeAll { $0 === image }
                continuation.resume(returning: result)
            }
        }
    }

    private override init() {
        view = WKWebView(frame: .zero)
        super.init()
        view.isOpaque = false
        view.backgroundColor = .clear
        view.scrollView.backgroundColor = .clear
        view.scrollView.contentInsetAdjustmentBehavior = .never
        view.isUserInteractionEnabled = false
        view.navigationDelegate = self
    }

    private func draw(_ data: Data, side: CGFloat, done: @escaping (UIImage?) -> Void) {
        self.done = done
        // A web view only draws while it is in a window.
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        guard let window = scenes.flatMap(\.windows).first else { return finish(nil) }
        let ratio = SVGSize.ratio(of: data)
        let points = side / window.screen.scale
        let size = ratio >= 1 ? CGSize(width: points, height: points / ratio) : CGSize(width: points * ratio, height: points)
        view.frame = CGRect(origin: CGPoint(x: -2 * size.width, y: -2 * size.height), size: size)
        window.insertSubview(view, at: 0)
        let page = """
            <html><head><meta name="viewport" content="width=\(size.width), initial-scale=1"></head>\
            <body style="margin:0;background:transparent"><img style="display:block;width:100vw;height:100vh;object-fit:contain" \
            src="data:image/svg+xml;base64,\(data.base64EncodedString())"></body></html>
            """
        view.loadHTMLString(page, baseURL: nil)
        DispatchQueue.main.asyncAfter(deadline: .now() + 5) { [weak self] in self?.finish(nil) }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        let configuration = WKSnapshotConfiguration()
        configuration.afterScreenUpdates = true
        webView.takeSnapshot(with: configuration) { [weak self] image, _ in self?.finish(image) }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        finish(nil)
    }

    private func finish(_ image: UIImage?) {
        guard let done else { return }
        self.done = nil
        view.removeFromSuperview()
        done(image)
    }
}

/// The width of an SVG over its height, from its root's `width` and `height` or its `viewBox`.
final class SVGSize: NSObject, XMLParserDelegate {
    private var size: CGSize?

    static func ratio(of data: Data) -> CGFloat {
        size(of: data).map { $0.width / $0.height } ?? 1
    }

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
