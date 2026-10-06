#if os(iOS)
import SwiftUI

/// The settings over the whole window, under a top bar with the way back. A narrow window lists
/// the sections and opens one over the list; a wide one has the list beside the open section.
struct SettingsScreen: View {
    /// A window at least this wide has the sections beside the open one.
    private static let wide: CGFloat = 700

    @Environment(AppStore.self) private var store
    @State private var path: [SettingsSection] = []

    var body: some View {
        GeometryReader { window in
            NavigationStack(path: $path) {
                Group {
                    if window.size.width >= Self.wide {
                        SettingsSplit()
                    } else {
                        SettingsSidebar(pushes: true) { section in
                            store.openSettings(section, target: store.settingsTarget)
                            path = [section]
                        }
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.themeBackground.ignoresSafeArea())
                .navigationTitle(window.size.width >= Self.wide ? store.settings?.title ?? "Settings" : "Settings")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) { back }
                }
                .navigationDestination(for: SettingsSection.self) { section in
                    SettingsPage(section: section)
                        .background(Color.themeBackground.ignoresSafeArea())
                        .navigationTitle(section.title)
                        .navigationBarTitleDisplayMode(.inline)
                }
            }
            .onAppear { follow(wide: window.size.width >= Self.wide) }
            .onChange(of: store.settings) { follow(wide: window.size.width >= Self.wide) }
            .onChange(of: store.settingsTarget) { follow(wide: window.size.width >= Self.wide) }
        }
    }

    /// A narrow window opens the section the store was pointed at, unless that is only the list's
    /// first one.
    private func follow(wide: Bool) {
        guard !wide, let section = store.settings, section != .general || store.settingsTarget != nil, path.last != section else { return }
        path = [section]
    }

    private var back: some View {
        Button {
            store.closeSettings()
        } label: {
            Image(.chevronLeft, size: 16)
        }
        .keyboardShortcut(.cancelAction)
        .accessibilityLabel("Back to the threads")
    }
}
#endif
