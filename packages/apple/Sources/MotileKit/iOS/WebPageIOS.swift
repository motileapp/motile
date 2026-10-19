#if os(iOS)
import UIKit
import WebKit

/// A web view that leaves dragging to the transcript around it, since its page is as tall as it is
/// shown.
final class PageWebView: WKWebView {
    override init(frame: CGRect, configuration: WKWebViewConfiguration) {
        super.init(frame: frame, configuration: configuration)
        scrollView.isScrollEnabled = false
    }

    required init?(coder: NSCoder) { fatalError("not used") }
}

extension WKWebView {
    /// Shows the view behind it until the page draws its own background.
    func clearsBackground() {
        isOpaque = false
        backgroundColor = .clear
        scrollView.backgroundColor = .clear
        scrollView.contentInsetAdjustmentBehavior = .never
    }
}

extension WKWebViewConfiguration {
    /// A video plays where it is, once pressed, not full screen.
    func allowsPlayback() {
        allowsInlineMediaPlayback = true
        mediaTypesRequiringUserActionForPlayback = .all
    }
}

extension UIColor {
    /// The colour as CSS takes it in one appearance, `#rrggbbaa`.
    func css(dark: Bool) -> String {
        var (red, green, blue, alpha): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        guard Theme.resolved(self, dark: dark).getRed(&red, green: &green, blue: &blue, alpha: &alpha) else { return "" }
        return cssHex(red, green, blue, alpha)
    }
}
#endif
