#if os(iOS)
import UIKit
import WebKit

/// A web view whose page scrolls inside it only when it is taller than it is shown, and which
/// takes the fingers its page holds from the transcript and the drawer around it.
final class PageWebView: WKWebView {
    /// The page holds the fingers on it.
    private(set) var holdsTouch = false

    override init(frame: CGRect, configuration: WKWebViewConfiguration) {
        super.init(frame: frame, configuration: configuration)
        scrollView.isScrollEnabled = false
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func heard(_ message: WebPage.Message) {
        guard case .hold(let holds) = message, holds != holdsTouch else { return }
        holdsTouch = holds
        guard holds else { return }
        for scroll in sequence(first: superview, next: { $0?.superview }).compactMap({ $0 as? UIScrollView }) {
            scroll.panGestureRecognizer.isEnabled = false
            scroll.panGestureRecognizer.isEnabled = true
        }
    }
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
