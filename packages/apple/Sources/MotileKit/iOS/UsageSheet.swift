#if os(iOS)
import SwiftUI

/// What was done, what the agents spent and what is left of their plans, in a sheet: the tabs
/// pinned under its bar, which the page fades under, the period at the top of the page, and the
/// servers counted in the bar's menu once there are several. Pulling the page down reads it again.
struct UsageSheet: View {
    private static let margin: CGFloat = 16

    @Environment(AppStore.self) private var store
    private var model: UsageModel { store.usage }

    private var picksServers: Bool { store.usageServers.count > 1 }

    var body: some View {
        NavigationStack {
            UsageContent(model: model, margin: Self.margin, top: 4, periodInPage: true)
                .refreshable { await model.refreshed() }
                .pinnedOnTop {
                    UsageTabs(model: model, fills: true)
                        .padding(.horizontal, Self.margin)
                        .padding(.top, 4)
                        .padding(.bottom, 8)
                }
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
        .presentationSizing(.page)
        .presentationDragIndicator(.visible)
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
        .accessibilityLabel("Servers counted")
    }
}

private extension View {
    /// What stays over the top of the page while it scrolls: on the bar's glass edge where the
    /// system has one, which the page fades under, else on the page's own background.
    @ViewBuilder func pinnedOnTop(@ViewBuilder _ content: () -> some View) -> some View {
        if #available(iOS 26, *) {
            safeAreaBar(edge: .top, spacing: 0, content: content)
                .scrollEdgeEffectStyle(.soft, for: .top)
        } else {
            safeAreaInset(edge: .top, spacing: 0) { content().background(Color.themeBackground) }
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
