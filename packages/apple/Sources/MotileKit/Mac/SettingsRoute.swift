#if os(macOS)
import SwiftUI

/// The settings over the whole window: their sections on the left, the open one's page on the
/// right and its title in the window's top bar. The way back is in the top bar beside the
/// window's buttons.
struct SettingsRoute: View {
    let section: SettingsSection

    var body: some View {
        SettingsSplit()
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.themeBackground.ignoresSafeArea())
            .overlay(alignment: .topLeading) { topBar }
    }

    /// The page's title, in the middle of the top bar over it.
    private var topBar: some View {
        GeometryReader { proxy in
            Text(section.title)
                .font(.ui(size: 13, weight: .semibold))
                .lineLimit(1)
                .padding(.horizontal, 20)
                .frame(maxWidth: .infinity)
                .padding(.leading, SettingsSplit.sidebarWidth + 1)
                .frame(height: proxy.safeAreaInsets.top)
                .offset(y: -proxy.safeAreaInsets.top)
                .allowsHitTesting(false)
        }
    }
}
#endif
