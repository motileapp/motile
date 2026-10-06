#if os(iOS)
import SwiftUI

/// What the agents spent and what is left of their plans, in a sheet: "Usage" as its title, whose
/// menu picks the servers counted, and the choices over what they show.
struct UsageSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var model = UsageModel()

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                UsageControls(model: model)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                UsageContent(model: model)
            }
            .background(Color.themeBackground.ignoresSafeArea())
            .navigationTitle("Usage")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarTitleMenu { UsageServerPicks(model: model) }
            .modifier(Subtitle(text: model.serversLabel(among: store.servers)))
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { SheetCloseButton() }
            }
        }
        .presentationSizing(.page)
        .presentationDragIndicator(.visible)
    }
}

private struct Subtitle: ViewModifier {
    let text: String

    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content.navigationSubtitle(text)
        } else {
            content
        }
    }
}
#endif
