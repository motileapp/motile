#if os(iOS)
import UIKit
import WebKit

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
