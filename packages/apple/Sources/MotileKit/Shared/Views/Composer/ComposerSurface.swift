import SwiftUI

extension View {
    /// The surface of the composer and its strips. On the Mac it is the composer's colour with a
    /// hairline around it. On iOS it is Liquid Glass under a tint where the system has it, which
    /// draws its own edge, and the system's material with a hairline before that.
    func composerSurface<S: Shape>(in shape: S) -> some View {
        composerFill(in: shape).environment(\.surface, .composer)
    }

    /// The small shadow the composer's box casts on its strips.
    func composerBoxShadow() -> some View {
        #if os(macOS)
        shadow(color: .themeComposerShadow, radius: 3)
        #else
        self
        #endif
    }

    /// The wide shadow around the composer and its strips together.
    func composerOutlineShadow() -> some View {
        #if os(macOS)
        shadow(color: .themeComposerOutlineShadow, radius: 16, y: 8)
        #else
        self
        #endif
    }

    @ViewBuilder
    private func composerFill<S: Shape>(in shape: S) -> some View {
        #if os(macOS)
        background(Color.themeComposer, in: shape)
            .overlay {
                shape.stroke(Color.themeBorder, lineWidth: 1)
                shape.stroke(LinearGradient(colors: [.themeComposerEdge, .clear], startPoint: .top, endPoint: .center), lineWidth: 1)
            }
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
