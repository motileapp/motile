#if os(macOS)
import AppKit
import WebKit

/// A web view that leaves the wheel to the transcript around it, since its page is as tall as it is
/// shown.
final class PageWebView: WKWebView {
    override func scrollWheel(with event: NSEvent) {
        nextResponder?.scrollWheel(with: event)
    }
}

extension WKWebView {
    /// Shows the view behind it until the page draws its own background.
    func clearsBackground() {
        setValue(false, forKey: "drawsBackground")
    }
}

extension WKWebViewConfiguration {
    /// A video plays where it is, once pressed.
    func allowsPlayback() {
        mediaTypesRequiringUserActionForPlayback = .all
    }
}

extension NSColor {
    /// The colour as CSS takes it in one appearance, `#rrggbbaa`.
    func css(dark: Bool) -> String {
        guard let color = Theme.resolved(self, dark: dark).usingColorSpace(.sRGB) else { return "" }
        return cssHex(color.redComponent, color.greenComponent, color.blueComponent, color.alphaComponent)
    }
}
#endif
