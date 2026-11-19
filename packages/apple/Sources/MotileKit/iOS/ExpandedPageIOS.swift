#if os(iOS)
import SwiftUI

/// An HtmlView's page over the whole screen, with the chip that puts it back under it, where the
/// thumb is, as the chip under the page in the transcript opened it. A page shorter than the screen
/// is only as tall as its content, so that the chip stays by it, and both sit at the bottom. Every
/// touch on the page is the page's: no pull closes it, and the system's gestures at the screen's
/// edges wait for a second swipe.
struct ExpandedPageCover: View {
    @Environment(AppStore.self) private var store
    let page: ExpandedPage
    @State private var content: CGFloat?

    var body: some View {
        VStack(spacing: ProgramWeb.exitGap) {
            ExpandedPageContent(page: page) { height in
                guard abs(height - (content ?? 0)) > 0.5 else { return }
                content = height
            }
            .frame(maxHeight: content ?? .infinity)
            ActionButton(
                "Exit full screen", icon: .minimize2, variant: .outline, size: ProgramWeb.exitSize, symbolSize: ProgramWeb.exitSymbol, round: true
            ) { store.closeExpandedPage() }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
        .background(Color.themeBackground.ignoresSafeArea())
        .defersSystemGestures(on: .all)
    }
}
#endif
