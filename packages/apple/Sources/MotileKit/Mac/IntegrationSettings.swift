#if os(macOS)
import SwiftUI

/// The integrations' page of the settings: a switch for each.
struct IntegrationSettings: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        SettingsGroup("integrations", "Issue trackers", caption: "Each one that is on has a tab in the right panel.") {
            ForEach(Integration.allCases) { integration in
                SettingsRow {
                    HStack(spacing: 12) {
                        IntegrationLogo(integration: integration)
                        SettingsLabel(integration.title, description: integration.summary)
                    }
                } trailing: {
                    Switch(isOn: Binding { store.isOn(integration) } set: { store.setIntegration(integration, on: $0) })
                }
                .id("integration-\(integration.rawValue)")
                if integration != Integration.allCases.last { ThemeDivider() }
            }
        }
    }
}
#endif
