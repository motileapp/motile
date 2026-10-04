import SwiftUI

extension View {
    /// The surface of the composer and its strips. On the Mac it is the composer's colour with a
    /// hairline around it. On iOS it is Liquid Glass under a tint where the system has it, which
    /// draws its own edge, and the system's material with a hairline before that.
    @ViewBuilder
    func composerSurface<S: Shape>(in shape: S) -> some View {
        #if os(macOS)
        background(Color.themeComposer, in: shape)
            .overlay { shape.stroke(Color.themeBorder, lineWidth: 1) }
        #else
        if #available(iOS 26.0, *) {
            background(Color.themeGlassTint, in: shape)
                .glassEffect(.regular, in: shape)
        } else {
            background(Color.themeGlassTint, in: shape)
                .background(.regularMaterial, in: shape)
                .overlay { shape.stroke(Color.themeStrongBorder, lineWidth: 1) }
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
