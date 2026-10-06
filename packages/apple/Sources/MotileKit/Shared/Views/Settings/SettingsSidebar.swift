import SwiftUI

/// The settings' sections under a search for one of them. While something is typed, the groups
/// found take the sections' place.
struct SettingsSidebar: View {
    @Environment(AppStore.self) private var store
    /// The section whose page is shown beside the list, lit in it.
    var selected: SettingsSection?
    /// The sections open as pages of their own, so each row points on.
    var pushes = false
    let choose: (SettingsSection) -> Void

    private static let rowMargin = EdgeInsets(top: 1, leading: sidebarRowInset, bottom: 1, trailing: sidebarRowInset)

    var body: some View {
        @Bindable var store = store
        VStack(spacing: 0) {
            SearchField(text: $store.settingsQuery)
                .padding(.horizontal, 10)
                .padding(.top, 2)
                .padding(.bottom, 6)
            ScrollView {
                LazyVStack(spacing: 0) {
                    if store.settingsQuery.isEmpty {
                        ForEach(SettingsSection.allCases) { section in
                            row(section)
                        }
                    } else {
                        results
                    }
                }
                .padding(.bottom, 12)
            }
            .scrollDismissesKeyboard(.immediately)
        }
    }

    private func row(_ section: SettingsSection) -> some View {
        HStack(spacing: 8) {
            Image(section.symbol, size: 14)
            Text(section.title)
                .font(.ui(size: 13, weight: .medium))
            Spacer(minLength: 0)
            if pushes {
                Image(.chevronRight, size: 13)
                    .foregroundStyle(Color.themeTertiary)
            }
        }
        .padding(.horizontal, 8)
        .frame(height: pressable(30))
        .padding(Self.rowMargin)
        .contentShape(Rectangle())
        .button(.highlight(radius: 8, selected: selected == section, inset: Self.rowMargin, faded: true)) { choose(section) }
    }

    @ViewBuilder private var results: some View {
        let found = SettingsEntry.matching(store.settingsQuery)
        ForEach(found) { entry in
            HStack(spacing: 8) {
                Image(entry.section.symbol, size: 14)
                VStack(alignment: .leading, spacing: 1) {
                    Text(entry.title)
                        .font(.ui(size: 13, weight: .medium))
                    Text(entry.section.title)
                        .font(.ui(size: 11))
                        .foregroundStyle(Color.themeTertiary)
                }
                .lineLimit(1)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .frame(minHeight: pressable(30))
            .padding(.vertical, 4)
            .padding(Self.rowMargin)
            .contentShape(Rectangle())
            .button(.highlight(radius: 8, inset: Self.rowMargin, faded: true)) {
                store.settingsTarget = entry.id
                choose(entry.section)
            }
        }
        if found.isEmpty {
            Text("No settings found")
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeTertiary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, sidebarRowInset + 8)
                .padding(.vertical, 10)
        }
    }
}

/// The sections beside the page of the one that is open, as on a Mac or an iPad. The sections
/// are as wide as the sidebar of the threads, behind the same `divider`, so nothing shifts
/// between the two.
struct SettingsSplit<Divider: View>: View {
    @Environment(AppStore.self) private var store
    let sidebarWidth: CGFloat
    @ViewBuilder let divider: () -> Divider

    var body: some View {
        HStack(spacing: 0) {
            SettingsSidebar(selected: store.settings) { store.openSettings($0, target: store.settingsTarget) }
                .frame(width: sidebarWidth)
            divider()
            SettingsPage(section: store.settings ?? .general)
                .frame(maxWidth: .infinity)
        }
    }
}
