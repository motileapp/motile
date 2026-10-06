#if os(macOS)
import SwiftUI

/// What the agents spent and what is left of their plans, in place of the thread: the title and
/// the choices in the top bar over it, after the way back.
struct UsageRoute: View {
    @Environment(AppStore.self) private var store
    /// How far the title starts from the pane's left edge: past the window's buttons when the
    /// sidebar is hidden.
    let titleInset: CGFloat

    @State private var model = UsageModel()

    var body: some View {
        GeometryReader { window in
            UsageContent(model: model, top: 36)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.themeBackground.ignoresSafeArea())
                .overlay(alignment: .topLeading) {
                    HStack(spacing: 12) {
                        HStack(spacing: ControlSize.regular.padding) {
                            ActionButton("Back", icon: .arrowLeft, help: "Back to the threads (Esc)", variant: .ghost) { store.closeRoute() }
                                .keyboardShortcut(.cancelAction)
                                .padding(.horizontal, -ControlSize.regular.padding)
                                .padding(.leading, ControlSize.regular.symbolOutset)
                            Rectangle()
                                .fill(Color.themeBorderSecondary)
                                .frame(width: 1, height: 14)
                            UsageTitle(model: model)
                        }
                        Spacer(minLength: 12)
                        UsageControls(model: model)
                    }
                    .padding(.leading, titleInset)
                    .padding(.trailing, 14)
                    .frame(height: window.safeAreaInsets.top)
                    .offset(y: -window.safeAreaInsets.top)
                }
        }
    }
}
#endif
