import SwiftUI

extension View {
    /// The surface of the composer and its strips: a card. On the Mac it is the card's colour
    /// with a hairline around it. On iOS it is Liquid Glass in the card's tint where the system
    /// has it, which draws its own edge, and the system's material with a hairline before that.
    func composerSurface<S: Shape>(in shape: S) -> some View {
        composerFill(in: shape).environment(\.surface, .card)
    }

    /// The shadow the composer's box casts on its strips.
    func composerBoxShadow() -> some View {
        #if os(macOS)
        shadow(.regular)
        #else
        self
        #endif
    }

    /// The shadow around the composer and its strips together.
    func composerOutlineShadow() -> some View {
        #if os(macOS)
        shadow(.stronger)
        #else
        self
        #endif
    }

    @ViewBuilder
    private func composerFill<S: Shape>(in shape: S) -> some View {
        #if os(macOS)
        background(Color.themeCard, in: shape)
            .overlay { shape.stroke(Color.themeBorderCard, lineWidth: 1) }
        #else
        if #available(iOS 26.0, *) {
            glassEffect(.regular.tint(Color.themeCard), in: shape)
        } else {
            background(.regularMaterial, in: shape)
                .overlay { shape.stroke(Color.themeBorderCard, lineWidth: 1) }
        }
        #endif
    }
}

/// Draws the glass surfaces inside it as one piece of glass.
struct GlassGroup<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        if #available(iOS 26.0, macOS 26.0, *) {
            GlassEffectContainer(spacing: 0) { content }
        } else {
            content
        }
    }
}
