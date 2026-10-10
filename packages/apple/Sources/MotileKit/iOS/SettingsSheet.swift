#if os(iOS)
import SwiftUI

/// The settings in a sheet, with a close button. A narrow sheet lists the sections and pushes the
/// open one over the list; a wide one has the list beside it.
struct SettingsSheet: View {
    /// A sheet at least this wide has the sections beside the open one.
    private static let wide: CGFloat = 700

    @Environment(AppStore.self) private var store
    @State private var path: [SettingsSection] = []

    var body: some View {
        GeometryReader { sheet in
            let wide = sheet.size.width >= Self.wide
            Group {
                if wide {
                    split
                } else {
                    stack
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

    private var stack: some View {
        NavigationStack(path: $path) {
            SettingsSections { path = [$0] }
                .navigationTitle("Settings")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarTrailing) { SheetCloseButton() }
                }
                .navigationDestination(for: SettingsSection.self) { section in
                    page(section)
                }
        }
        .onChange(of: path) {
            guard let section = path.last, section != store.settings else { return }
            store.openSettings(section, target: store.settingsTarget)
        }
    }

    private var split: some View {
        NavigationSplitView(columnVisibility: .constant(.all)) {
            SettingsSections(selected: store.settings ?? .general) { store.openSettings($0, target: store.settingsTarget) }
                .navigationTitle("Settings")
                .navigationBarTitleDisplayMode(.inline)
                .navigationSplitViewColumnWidth(MainScreen.sidebarWidth)
                .toolbar(removing: .sidebarToggle)
        } detail: {
            NavigationStack {
                page(store.settings ?? .general)
                    .toolbar {
                        ToolbarItem(placement: .topBarTrailing) { SheetCloseButton() }
                    }
            }
            .id(store.settings)
        }
        .navigationSplitViewStyle(.balanced)
    }

    private func page(_ section: SettingsSection) -> some View {
        SettingsPage(section: section)
            .navigationTitle(section.title)
            .navigationBarTitleDisplayMode(.inline)
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
