import SwiftUI

/// What is under way: Lucide's loader, turning, in the colour of the text around it.
struct Spinner: View {
    var size: CGFloat = ControlSize.regular.symbol
    @State private var turned = false

    var body: some View {
        Image(.loader, size: size)
            .rotationEffect(.degrees(turned ? 360 : 0))
            .animation(.linear(duration: 1.2).repeatForever(autoreverses: false), value: turned)
            .onAppear { turned = true }
            .accessibilityLabel("Working")
    }
}
