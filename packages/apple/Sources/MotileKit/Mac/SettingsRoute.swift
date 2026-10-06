#if os(macOS)
import SwiftUI

/// The settings over the whole window: their sections on the left, as wide as the sidebar and
/// dragged with the same line, the open one's page on the right and its title in the window's
/// top bar. The way back is in the top bar beside the window's buttons.
struct SettingsRoute: View {
    let section: SettingsSection
    @AppStorage(MainView.sidebarWidthKey) private var sidebarWidth = 280.0

    var body: some View {
        GeometryReader { window in
            let widths = MainView.sidebarWidths(in: Double(window.size.width))
            let shownWidth = min(widths.upperBound, max(widths.lowerBound, sidebarWidth))
            SettingsSplit(sidebarWidth: shownWidth) {
                PaneDivider(width: $sidebarWidth, widths: widths)
                    .zIndex(1)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.themeBackground.ignoresSafeArea())
            .overlay(alignment: .topLeading) { topBar(window, pastSidebar: shownWidth + 1) }
        }
    }

    /// The page's title, in the middle of the top bar over it.
    private func topBar(_ window: GeometryProxy, pastSidebar: CGFloat) -> some View {
        Text(section.title)
            .font(.ui(size: 13, weight: .semibold))
            .lineLimit(1)
            .padding(.horizontal, 20)
            .frame(maxWidth: .infinity)
            .padding(.leading, pastSidebar)
            .frame(height: window.safeAreaInsets.top)
            .offset(y: -window.safeAreaInsets.top)
            .allowsHitTesting(false)
    }
}
#endif
