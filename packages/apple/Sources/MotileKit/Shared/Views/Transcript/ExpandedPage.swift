import SwiftUI

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// An HtmlView's page over the whole window. The web view the row showed moves here and back, so
/// that the page keeps where it was.
final class ExpandedPage: Identifiable {
    let title: String
    let page: WebPageView
    private let back: (WebPageView) -> Void

    init(title: String, page: WebPageView, back: @escaping (WebPageView) -> Void) {
        self.title = title
        self.page = page
        self.back = back
    }

    /// Gives the page back to its row.
    func close() {
        back(page)
    }
}

/// Holds the expanded page's web view, which fills it.
final class ExpandedPageHolder: FlippedView {
    var page: WebPageView? {
        didSet {
            guard let page else { return }
            addSubview(page)
            page.fills = true
            relayout()
        }
    }

    /// Only while the page is here: the row takes it back while the cover still goes away.
    override func layoutNow() {
        guard let page, page.superview === self else { return }
        page.frame = bounds
    }
}

#if os(macOS)
struct ExpandedPageContent: NSViewRepresentable {
    let page: ExpandedPage
    /// How tall the page's content is drawn.
    var onContent: (CGFloat) -> Void = { _ in }

    func makeNSView(context: Context) -> ExpandedPageHolder { ExpandedPageHolder() }

    func updateNSView(_ holder: ExpandedPageHolder, context: Context) {
        if holder.page !== page.page { holder.page = page.page }
        page.page.onContent = onContent
    }
}
#else
struct ExpandedPageContent: UIViewRepresentable {
    let page: ExpandedPage
    /// How tall the page's content is drawn.
    var onContent: (CGFloat) -> Void = { _ in }

    func makeUIView(context: Context) -> ExpandedPageHolder { ExpandedPageHolder() }

    func updateUIView(_ holder: ExpandedPageHolder, context: Context) {
        if holder.page !== page.page { holder.page = page.page }
        page.page.onContent = onContent
    }
}
#endif
