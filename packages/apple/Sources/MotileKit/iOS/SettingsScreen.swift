#if os(iOS)
import SwiftUI

/// Where the settings are in their stack: the list of sections, or one section's page over it.
enum SettingsRoute: Hashable {
    case settings
    case section(SettingsSection)
}

/// The stack the settings are pushed on, laid over the threads while they are open. Its root is
/// empty and clear, so the settings come in as a page does and a swipe from the edge takes them
/// away. It can't hold the threads: their own stacks would hand their bars up to it.
struct SettingsStack: View {
    /// A window at least this wide has the sections beside the open one.
    private static let wide: CGFloat = 700

    @Environment(AppStore.self) private var store
    @State private var path = NavigationPath()
    /// What the path holds.
    @State private var routes: [SettingsRoute] = []

    var body: some View {
        GeometryReader { window in
            let wide = window.size.width >= Self.wide
            NavigationStack(path: $path) {
                Color.clear
                    .containerBackground(.clear, for: .navigation)
                    .toolbar(.hidden, for: .navigationBar)
                    .navigationDestination(for: SettingsRoute.self) { route in
                        switch route {
                        case .settings:
                            SettingsScreen(wide: wide) { set([.settings, .section($0)]) }
                        case .section(let section):
                            SettingsPage(section: section)
                                .background(Color.themeBackground.ignoresSafeArea())
                                .navigationTitle(section.title)
                                .navigationBarTitleDisplayMode(.inline)
                        }
                    }
            }
            // A path set as the stack appears isn't animated; set a moment later, it is pushed.
            .onAppear { DispatchQueue.main.async { follow(wide: wide) } }
            .onChange(of: store.settings) { follow(wide: wide) }
            .onChange(of: store.settingsTarget) { follow(wide: wide) }
            .onChange(of: wide) { _, wide in follow(wide: wide) }
            .onChange(of: path.count) { _, count in
                routes = Array(routes.prefix(count))
                guard count == 0, store.settings != nil else { return }
                store.closeSettings()
            }
        }
    }

    private func set(_ wanted: [SettingsRoute]) {
        routes = wanted
        path = NavigationPath(wanted)
    }

    /// A narrow window opens the section the store was pointed at, unless that is only the list's
    /// first one.
    private func follow(wide: Bool) {
        guard let section = store.settings else { return }
        if !wide && (section != .general || store.settingsTarget != nil) {
            guard routes.last != .section(section) else { return }
            set([.settings, .section(section)])
        } else if routes.isEmpty || wide && routes.count > 1 {
            set([.settings])
        }
    }
}

/// The list of sections, or in a wide window the list beside the open section.
private struct SettingsScreen: View {
    @Environment(AppStore.self) private var store
    let wide: Bool
    let push: (SettingsSection) -> Void

    var body: some View {
        Group {
            if wide {
                SettingsSplit(sidebarWidth: MainScreen.sidebarWidth) { MainScreen.line }
            } else {
                SettingsSidebar(pushes: true) { section in
                    store.openSettings(section, target: store.settingsTarget)
                    push(section)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.themeBackground.ignoresSafeArea())
        .navigationTitle(wide ? store.settings?.title ?? "Settings" : "Settings")
        .navigationBarTitleDisplayMode(.inline)
    }
}
#endif
