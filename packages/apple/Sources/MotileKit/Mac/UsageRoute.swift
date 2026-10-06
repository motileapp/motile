#if os(macOS)
import SwiftUI

/// What the agents spent and what is left of their plans, over the whole window: the title and
/// the choices in the window's top bar, past the way back.
struct UsageRoute: View {
    /// How far the title starts from the window's left edge: past its buttons and the way back.
    private static let titleInset: CGFloat = 176

    @State private var model = UsageModel()

    var body: some View {
        GeometryReader { window in
            UsageContent(model: model)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.themeBackground.ignoresSafeArea())
                .overlay(alignment: .topLeading) {
                    HStack(spacing: 12) {
                        UsageTitle(model: model)
                        Spacer(minLength: 12)
                        UsageControls(model: model)
                    }
                    .padding(.leading, Self.titleInset)
                    .padding(.trailing, 14)
                    .frame(height: window.safeAreaInsets.top)
                    .offset(y: -window.safeAreaInsets.top)
                }
        }
    }
}
#endif
