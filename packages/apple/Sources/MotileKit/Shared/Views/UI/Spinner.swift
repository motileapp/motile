import SwiftUI

/// What is under way: Lucide's loader, turning, in the colour of the text around it.
struct Spinner: View {
    /// The loader reaches every side of its square, so it is drawn smaller in it to look as
    /// large as the symbols it stands in for.
    static let fill: CGFloat = 0.85
    static let turn = 1.2

    var size: CGFloat = ControlSize.regular.symbol
    @State private var turned = false

    var body: some View {
        let side = PlatformImage.symbolSide(size)
        Image(.loader, size: size * Self.fill)
            .frame(width: side, height: side)
            .animation(.linear(duration: Self.turn).repeatForever(autoreverses: false)) { loader in
                loader.rotationEffect(.degrees(turned ? 360 : 0))
            }
            .onAppear { turned = true }
            .accessibilityLabel("Working")
    }
}

/// A symbol that turns while `turning`, and finishes the turn it is on once that ends.
struct TurningSymbol: View {
    let symbol: Symbol
    var size: CGFloat = ControlSize.regular.symbol
    let turning: Bool
    @State private var shown = Shown()
    @State private var changes = 0

    /// The angle last drawn, which a change of `turning` goes on from.
    private final class Shown { var angle = 0.0 }

    var body: some View {
        Image(symbol, size: size)
            .keyframeAnimator(initialValue: 0.0, trigger: changes) { image, angle in
                shown.angle = angle
                return image.rotationEffect(.degrees(angle))
            } keyframes: { _ in
                let from = shown.angle
                let to = turning ? from + 360 * 10_000 : (from / 360).rounded(.up) * 360
                if to > from {
                    LinearKeyframe(to, duration: (to - from) / 360 * Spinner.turn)
                } else {
                    MoveKeyframe(to)
                }
            }
            .onChange(of: turning, initial: true) { changes += 1 }
    }
}
