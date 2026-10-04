#if os(iOS)
import UIKit
import WebKit

/// Draws an SVG file, which many projects have for an icon and iOS has nothing to read with but
/// a web view: one that is never seen draws it, and a picture is taken of that.
@MainActor
final class SVGImage: NSObject, WKNavigationDelegate {
    private static let side: CGFloat = 256
    private static var drawing: [SVGImage] = []

    private let view: WKWebView
    private var done: ((UIImage?) -> Void)?

    nonisolated static func isSVG(_ data: Data) -> Bool {
        guard let start = String(data: data.prefix(600), encoding: .utf8) else { return false }
        return start.contains("<svg")
    }

    static func image(of data: Data) async -> UIImage? {
        await withCheckedContinuation { continuation in
            let image = SVGImage()
            drawing.append(image)
            image.draw(data) { result in
                drawing.removeAll { $0 === image }
                continuation.resume(returning: result)
            }
        }
    }

    private override init() {
        view = WKWebView(frame: CGRect(x: -2 * Self.side, y: -2 * Self.side, width: Self.side, height: Self.side))
        super.init()
        view.isOpaque = false
        view.backgroundColor = .clear
        view.scrollView.backgroundColor = .clear
        view.isUserInteractionEnabled = false
        view.navigationDelegate = self
    }

    private func draw(_ data: Data, done: @escaping (UIImage?) -> Void) {
        self.done = done
        // A web view only draws while it is in a window.
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        guard let window = scenes.flatMap(\.windows).first else { return finish(nil) }
        window.insertSubview(view, at: 0)
        let page = """
            <html><head><meta name="viewport" content="width=\(Int(Self.side)), initial-scale=1"></head>\
            <body style="margin:0;background:transparent"><img style="width:100%;height:100%;object-fit:contain" \
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
#endif
