import SwiftUI

/// Content that scrolls once it is taller than `maxHeight`, fading out at an edge with more behind it.
struct FadingScroll<Content: View>: View {
    let maxHeight: CGFloat
    var anchor: UnitPoint = .top
    @ViewBuilder let content: Content

    @State private var hidden = HiddenEdges()

    private static var fade: CGFloat { scaled(24) }

    var body: some View {
        ViewThatFits(in: .vertical) {
            content
            ScrollView { content }
                .defaultScrollAnchor(anchor)
                .modifier(TracksHiddenEdges(hidden: $hidden))
                .mask { mask }
        }
        .frame(maxHeight: maxHeight)
    }

    private var mask: some View {
        VStack(spacing: 0) {
            LinearGradient(colors: [hidden.top ? .clear : .black, .black], startPoint: .top, endPoint: .bottom)
                .frame(height: Self.fade)
            Color.black
            LinearGradient(colors: [.black, hidden.bottom ? .clear : .black], startPoint: .top, endPoint: .bottom)
                .frame(height: Self.fade)
        }
    }
}

private struct HiddenEdges: Equatable {
    var top = false
    var bottom = false
}

private struct TracksHiddenEdges: ViewModifier {
    @Binding var hidden: HiddenEdges

    func body(content: Content) -> some View {
        if #available(macOS 15, *) {
            content.onScrollGeometryChange(for: HiddenEdges.self) { geometry in
                let offset = geometry.contentOffset.y + geometry.contentInsets.top
                let below = geometry.contentSize.height - offset - geometry.containerSize.height
                return HiddenEdges(top: offset > 0.5, bottom: below > 0.5)
            } action: { _, edges in
                hidden = edges
            }
        } else {
            content
        }
    }
}

extension View {
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
