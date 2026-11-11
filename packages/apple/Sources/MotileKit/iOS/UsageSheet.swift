#if os(iOS)
import SwiftUI

/// What was done, what the agents spent and what is left of their plans, in a sheet: the tabs in
/// the system's tab bar, the period at the top of each page, and the servers counted in the bar's
/// menu once there are several. Pulling a page down reads it again.
struct UsageSheet: View {
    private static let margin: CGFloat = 16

    @Environment(AppStore.self) private var store
    private var model: UsageModel { store.usage }

    private var picksServers: Bool { store.usageServers.count > 1 }

    var body: some View {
        TabView(selection: Bindable(model).tab) {
            ForEach(UsageModel.Tab.allCases) { tab in
                Tab(value: tab) {
                    page(tab)
                } label: {
                    Label {
                        Text(tab.label)
                    } icon: {
                        Image(platform: .symbol(tab.symbol, size: .lg))
                    }
                }
            }
        }
        .tint(Color.themeForeground)
        .minimizesOnScroll()
        .onAppear { model.start(store) }
        .presentationSizing(.page)
        .presentationDragIndicator(.visible)
    }

    private func page(_ tab: UsageModel.Tab) -> some View {
        NavigationStack {
            UsageContent(model: model, shown: tab, margin: Self.margin, top: 4, periodInPage: true)
                .refreshable { await model.refreshed() }
                .dropdowns()
                .background(Color.themeBackground.ignoresSafeArea())
                .navigationTitle("Usage")
                .navigationBarTitleDisplayMode(.inline)
                .serversSubtitle(picksServers ? model.serversLabel(among: store.usageServers) : nil)
                .toolbar {
                    GlassToolbarItem(placement: .topBarLeading) { SheetCloseButton() }
                    if picksServers {
                        GlassToolbarItem(placement: .topBarTrailing) { servers }
                    }
                }
        }
    }

    /// The servers counted, each checked or not.
    private var servers: some View {
        Menu {
            ForEach(model.serverChecks(among: store.usageServers)) { check in
                Toggle(check.title, isOn: Binding { check.checked } set: { _ in check.toggle() })
            }
        } label: {
            Image(.server, size: .lg)
        }
        .glassSymbolFrame()
        .accessibilityLabel("Servers counted")
    }
}

private extension View {
    @ViewBuilder func minimizesOnScroll() -> some View {
        if #available(iOS 26, *) {
            tabBarMinimizeBehavior(.onScrollDown)
        } else {
            self
        }
    }

    @ViewBuilder func serversSubtitle(_ subtitle: String?) -> some View {
        if #available(iOS 26, *), let subtitle {
            navigationSubtitle(subtitle)
        } else {
            self
        }
    }
}
#endif
