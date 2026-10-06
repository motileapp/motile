#if os(iOS)
import SwiftUI

/// The settings in a sheet, with a Done button. A narrow sheet lists the sections and pushes the
/// open one over the list; a wide one has the list beside it.
struct SettingsSheet: View {
    /// A sheet at least this wide has the sections beside the open one.
    private static let wide: CGFloat = 700

    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var path: [SettingsSection] = []

    var body: some View {
        GeometryReader { sheet in
            let wide = sheet.size.width >= Self.wide
            NavigationStack(path: $path) {
                Group {
                    if wide {
                        SettingsSplit(sidebarWidth: MainScreen.sidebarWidth) { MainScreen.line }
                    } else {
                        SettingsSidebar(pushes: true) { section in
                            store.openSettings(section, target: store.settingsTarget)
                            path = [section]
                        }
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .navigationTitle(wide ? store.settings?.title ?? "Settings" : "Settings")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Done") { dismiss() }
                    }
                }
                .navigationDestination(for: SettingsSection.self) { section in
                    SettingsPage(section: section)
                        .background(Color.themeBackground.ignoresSafeArea())
                        .navigationTitle(section.title)
                        .navigationBarTitleDisplayMode(.inline)
                }
            }
            .onAppear { follow(wide: wide) }
            .onChange(of: store.settings) { follow(wide: wide) }
            .onChange(of: store.settingsTarget) { follow(wide: wide) }
            .onChange(of: wide) { _, wide in follow(wide: wide) }
        }
        .presentationSizing(.page)
        .presentationDragIndicator(.visible)
    }

    /// A narrow sheet opens the section the store was pointed at, unless that is only the list's
    /// first one.
    private func follow(wide: Bool) {
        guard !wide else { return path = [] }
        guard let section = store.settings, section != .general || store.settingsTarget != nil, path.last != section else { return }
        path = [section]
    }
}
#endif
