import CMotileCore
import Foundation
import WebKit

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A page core built for a web view (`web::Page`): an HtmlView's program, or YouTube's player.
struct WebPage: Decodable {
    struct Heights: Decodable {
        let start: CGFloat
        let min: CGFloat
        let max: CGFloat
    }

    let html: String
    /// The address it is loaded at; none for a program's page, which is served from an origin of its own.
    let base: String?
    /// The height reporter, which runs in a world of its own out of the page's reach.
    let script: String?
    let handler: String
    let height: Heights

    static let heightMessage = "iui-html-view-height"

    /// A program's page in the text style around it: `color` its text, `background` behind it, as
    /// they are in the dark or the light.
    static func html(_ html: String, dark: Bool, textSize: CGFloat, lineHeight: CGFloat, color: PlatformColor, background: PlatformColor) -> WebPage? {
        build([
            "kind": "html",
            "html": html,
            "bridge": "web_kit",
            "dark": dark,
            "text_size": textSize,
            "line_height": lineHeight,
            "color": color.css(dark: dark),
            "background": background.css(dark: dark),
        ])
    }

    /// YouTube's player for an embed address core resolved.
    static func youtube(url: String, title: String) -> WebPage? {
        build(["kind": "youtube", "url": url, "title": title])
    }

    /// Asks core for the page: cheap, but called off the main thread like the canvas layouts.
    private static func build(_ request: [String: Any]) -> WebPage? {
        guard let data = try? JSONSerialization.data(withJSONObject: request), let json = String(data: data, encoding: .utf8) else { return nil }
        guard let answer = json.withCString({ motile_ui_page($0) }) else { return nil }
        defer { motile_free_string(answer) }
        return try? JSONDecoder().decode(WebPage.self, from: Data(String(cString: answer).utf8))
    }

    /// The height a message of the reporter gives, within the page's bounds.
    func height(from body: Any) -> CGFloat? {
        guard let text = body as? String, let data = text.data(using: .utf8) else { return nil }
        guard let message = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        guard message["type"] as? String == Self.heightMessage, let value = message["height"] as? Double, value.isFinite else { return nil }
        return Swift.min(Swift.max(CGFloat(value), height.min), height.max)
    }
}

/// The heights pages reported, by row and node, so that a page that comes back on screen is as
/// tall as it was and the row is measured at that height.
enum WebHeights {
    private static let lock = NSLock()
    private static var heights: [String: CGFloat] = [:]

    static func height(row: String, node: String) -> CGFloat? {
        lock.withLock { heights["\(row)/\(node)"] }
    }

    static func set(_ height: CGFloat, row: String, node: String) {
        lock.withLock { heights["\(row)/\(node)"] = height }
    }
}

/// A page in a sandboxed web view: loaded once, from an origin of its own (or the player's https
/// address), with no navigation, no other windows, a data store that keeps nothing, and one
/// channel, in a world of its own, for its height. YouTube's links open in the browser.
final class WebPageView: FlippedView, WKNavigationDelegate, WKUIDelegate {
    /// Called on the main thread with each height the page reports.
    var onHeight: ((CGFloat) -> Void)?

    private let page: WebPage
    private let player: Bool
    private let address: URL
    private var webView: WKWebView!
    private var loaded = false

    private static let scheme = "motile-page"

    init(page: WebPage) {
        self.page = page
        player = page.base != nil
        address = page.base.flatMap(URL.init(string:)) ?? URL(string: "\(Self.scheme)://\(UUID().uuidString.lowercased())/")!
        super.init(frame: .zero)
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        configuration.allowsPlayback()
        if !player {
            configuration.setURLSchemeHandler(PageSchemeHandler(html: page.html), forURLScheme: Self.scheme)
        }
        if let script = page.script {
            let world = WKContentWorld.world(name: "motile")
            let controller = configuration.userContentController
            controller.addUserScript(WKUserScript(source: script, injectionTime: .atDocumentStart, forMainFrameOnly: true, in: world))
            controller.add(HeightHandler(view: self), contentWorld: world, name: page.handler)
        }
        webView = WKWebView(frame: bounds, configuration: configuration)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.clearsBackground()
        addSubview(webView)
        load()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func layout() {
        super.layout()
        webView.frame = bounds
    }
    #else
    override func layoutSubviews() {
        super.layoutSubviews()
        webView.frame = bounds
    }
    #endif

    private func load() {
        if player {
            webView.loadHTMLString(page.html, baseURL: address)
        } else {
            webView.load(URLRequest(url: address))
        }
    }

    fileprivate func report(_ body: Any) {
        guard let height = page.height(from: body) else { return }
        onHeight?(height)
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction, decisionHandler: @escaping @MainActor @Sendable (WKNavigationActionPolicy) -> Void) {
        let url = action.request.url
        guard action.targetFrame?.isMainFrame != false else {
            decisionHandler(player && url.map(Self.playsInFrame) == true ? .allow : .cancel)
            return
        }
        if !loaded, url?.absoluteString == address.absoluteString {
            loaded = true
            decisionHandler(.allow)
            return
        }
        if player, action.navigationType == .linkActivated, let url, Self.isYouTube(url) { Platform.open(url) }
        decisionHandler(.cancel)
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        if player, let url = action.request.url, Self.isYouTube(url) { Platform.open(url) }
        return nil
    }

    private static func playsInFrame(_ url: URL) -> Bool {
        url.scheme == "about" || (url.scheme == "https" && url.host == "www.youtube-nocookie.com")
    }

    private static func isYouTube(_ url: URL) -> Bool {
        guard url.scheme == "https", let host = url.host else { return false }
        return host == "youtu.be" || host == "youtube.com" || host.hasSuffix(".youtube.com")
    }
}

/// Serves a program's page, and nothing else, at its view's own address.
private final class PageSchemeHandler: NSObject, WKURLSchemeHandler {
    private let data: Data

    init(html: String) {
        data = Data(html.utf8)
    }

    func webView(_ webView: WKWebView, start task: any WKURLSchemeTask) {
        guard let url = task.request.url else { return }
        let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: [
            "Content-Type": "text/html; charset=utf-8",
            "Content-Length": "\(data.count)",
        ])!
        task.didReceive(response)
        task.didReceive(data)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: any WKURLSchemeTask) {}
}

/// The page's one channel, which holds its view weakly: the content controller keeps its handlers.
private final class HeightHandler: NSObject, WKScriptMessageHandler {
    private weak var view: WebPageView?

    init(view: WebPageView) {
        self.view = view
    }

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
        guard message.frameInfo.isMainFrame else { return }
        view?.report(message.body)
    }
}

/// Components from 0 to 1 as `#rrggbbaa`.
func cssHex(_ components: CGFloat...) -> String {
    "#" + components.map { String(format: "%02x", Int((Swift.min(Swift.max($0, 0), 1) * 255).rounded())) }.joined()
}
