import SwiftUI

/// Content as tall as it is up to `maxHeight`, past which it scrolls, fading out at an edge
/// with more behind it. `padding` insets the content inside the scrolling edges.
struct FadingScroll<Content: View>: View {
    let maxHeight: CGFloat
    var anchor: UnitPoint = .top
    var padding: CGFloat = 0
    @ViewBuilder let content: Content

    @State private var contentHeight: CGFloat = 0

    var body: some View {
        ScrollView {
            content.background {
                GeometryReader { proxy in
                    Color.clear.onChange(of: proxy.size.height, initial: true) { contentHeight = proxy.size.height }
                }
            }
        }
        .contentMargins(padding, for: .scrollContent)
        .scrollBounceBehavior(.basedOnSize)
        .defaultScrollAnchor(anchor)
        .fadesHiddenEdges()
        .frame(height: min(contentHeight + padding * 2, maxHeight))
    }
}

/// How much of a scroll view's content is hidden past its top and its bottom.
private struct HiddenEdges: Equatable {
    var above: CGFloat = 0
    var below: CGFloat = 0
}

private struct FadesHiddenEdges: ViewModifier {
    @State private var hidden = HiddenEdges()

    func body(content: Content) -> some View {
        tracked(content).mask { mask }
    }

    @ViewBuilder
    private func tracked(_ content: Content) -> some View {
        if #available(macOS 15, *) {
            content.onScrollGeometryChange(for: HiddenEdges.self) { geometry in
                let above = geometry.contentOffset.y + geometry.contentInsets.top
                let below = geometry.contentSize.height + geometry.contentInsets.bottom - geometry.contentOffset.y - geometry.containerSize.height
                return HiddenEdges(above: min(EdgeFade.length, max(0, above)), below: min(EdgeFade.length, max(0, below)))
            } action: { _, edges in
                hidden = edges
            }
        } else {
            content
        }
    }

    private var mask: some View {
        VStack(spacing: 0) {
            LinearGradient(colors: [.black.opacity(1 - hidden.above / EdgeFade.length), .black], startPoint: .top, endPoint: .bottom)
                .frame(height: EdgeFade.length)
            Color.black
            LinearGradient(colors: [.black, .black.opacity(1 - hidden.below / EdgeFade.length)], startPoint: .top, endPoint: .bottom)
                .frame(height: EdgeFade.length)
        }
    }
}

extension View {
    /// Fades a scroll view's content out at an edge with more behind it, as a recycled list does.
    func fadesHiddenEdges() -> some View {
        modifier(FadesHiddenEdges())
    }

    /// Fades a scroll view's content out under the window's top bar, as the transcript does.
    func fadesUnderTopBar() -> some View {
        mask {
            GeometryReader { proxy in
                ZStack(alignment: .topTrailing) {
                    VStack(spacing: 0) {
                        Color.clear.frame(height: proxy.safeAreaInsets.top)
                        LinearGradient(colors: [.clear, .black], startPoint: .top, endPoint: .bottom)
                            .frame(height: 20)
                        Color.black
                    }
                    Color.black
                        .frame(width: TranscriptScroller.indicatorWidth)
                        .padding(.top, proxy.safeAreaInsets.top)
                }
                .ignoresSafeArea()
            }
        }
    }
}
