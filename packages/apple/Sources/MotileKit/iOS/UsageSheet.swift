#if os(iOS)
import SwiftUI

/// What the agents spent and what is left of their plans, in a sheet: the tabs across it, under
/// them the servers counted and the period, and the way to read it all again in its bar.
struct UsageSheet: View {
    private static let margin: CGFloat = 16

    @Environment(AppStore.self) private var store
    private var model: UsageModel { store.usage }

    private var picksServers: Bool { store.usageServers.count > 1 }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                VStack(spacing: 8) {
                    UsageTabs(model: model, fills: true)
                    if picksServers || model.tab != .limits {
                        HStack(spacing: 8) {
                            if picksServers {
                                UsageServersMenu(model: model)
                            }
                            if model.tab != .limits {
                                UsagePeriodMenu(model: model)
                            }
                            Spacer(minLength: 0)
                        }
                    }
                }
                .padding(.horizontal, Self.margin)
                .padding(.vertical, 8)
                UsageContent(model: model, margin: Self.margin)
            }
            .dropdowns()
            .background(Color.themeBackground.ignoresSafeArea())
            .navigationTitle("Usage")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { SheetCloseButton() }
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.refresh()
                    } label: {
                        TurningSymbol(symbol: .refreshCw, size: .lg, turning: model.refreshing)
                    }
                    .disabled(model.refreshing)
                    .accessibilityLabel("Refresh")
                }
            }
        }
        .presentationSizing(.page)
        .presentationDragIndicator(.visible)
    }
}
#endif
