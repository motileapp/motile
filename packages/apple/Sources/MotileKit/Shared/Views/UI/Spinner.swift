import SwiftUI

/// What is under way: Lucide's loader, turning, in the colour of the text around it.
struct Spinner: View {
    /// The loader reaches every side of its square, so it is drawn smaller in it to look as
    /// large as the symbols it stands in for.
    private static let fill: CGFloat = 0.85

    var size: CGFloat = ControlSize.regular.symbol
    @State private var turned = false

    var body: some View {
        let side = PlatformImage.symbolSide(size)
        Image(.loader, size: size * Self.fill)
            .frame(width: side, height: side)
            .animation(.linear(duration: 1.2).repeatForever(autoreverses: false)) { loader in
                loader.rotationEffect(.degrees(turned ? 360 : 0))
            }
            .onAppear { turned = true }
            .accessibilityLabel("Working")
    }
}
