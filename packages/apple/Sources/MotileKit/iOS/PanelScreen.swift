#if os(iOS)
import SwiftUI

/// What the Mac has in the panel beside the thread: the changes in the folder the thread works
/// in, its files, the files opened from either, and the agents the thread started. On a phone it
/// is under the thread, which a swipe to the left slides aside; in a wide window it is beside it.
struct PanelScreen: View {
    @Environment(AppStore.self) private var store
    /// The panel is a pane beside the thread, with its own buttons to cover the thread and to close.
    var beside = false

    var body: some View {
        let tabs = store.sidePanel.tabs
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                PanelTabStrip(tabs: tabs)
                if beside {
                    paneButtons
                }
            }
            .frame(height: 46)
            PanelLine()
            PanelContent(active: tabs.active)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.themeBackground.ignoresSafeArea())
        .navigationTitle(store.panelTarget?.name ?? "Files")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if !beside {
                ToolbarItem(placement: .topBarLeading) {
                    Button {
                        store.sidePanel.isOpen = false
                    } label: {
                        Image(.chevronLeft, size: 16)
                    }
                    .accessibilityLabel("Back")
                }
            }
        }
    }

    private var paneButtons: some View {
        HStack(spacing: 0) {
            let maximized = store.sidePanel.isMaximized
            IconOnlyButton(
                symbol: maximized ? .minimize2 : .maximize2,
                help: maximized ? "Restore the side panel" : "Maximize the side panel", size: 36, symbolSize: 14, faded: true
            ) {
                store.sidePanel.toggleMaximized()
            }
            IconOnlyButton(symbol: .x, help: "Close the side panel", size: 36, symbolSize: 14, faded: true) {
                store.sidePanel.isOpen = false
            }
        }
        .padding(.trailing, 6)
    }
}
#endif
