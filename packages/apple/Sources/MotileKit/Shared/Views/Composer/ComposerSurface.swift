import SwiftUI

extension View {
    /// The surface of the composer and its strips. On the Mac it is the composer's colour with a
    /// hairline around it. On iOS it is Liquid Glass under a tint where the system has it, which
    /// draws its own edge, and the system's material with a hairline before that.
    func composerSurface<S: Shape>(in shape: S) -> some View {
        composerFill(in: shape).environment(\.surface, .composer)
    }

    @ViewBuilder
    private func composerFill<S: Shape>(in shape: S) -> some View {
        #if os(macOS)
        background(Color.themeComposer, in: shape)
            .overlay { shape.stroke(Color.themeBorder, lineWidth: 1) }
        #else
        if #available(iOS 26.0, *) {
            background(Color.themeComposer.opacity(0.8), in: shape)
                .glassEffect(.regular, in: shape)
        } else {
            background(Color.themeComposer.opacity(0.8), in: shape)
                .background(.regularMaterial, in: shape)
                .overlay { shape.stroke(Color.themeBorderSecondary, lineWidth: 1) }
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
