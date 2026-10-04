import SwiftUI

extension View {
    /// The surface of what floats over the transcript: Liquid Glass under a tint where the system
    /// has it, which draws its own edge. Before that it is the system's material on iOS and a
    /// nearly opaque fill on the Mac, whose materials show what is behind the window, with a
    /// hairline around it.
    @ViewBuilder
    func glassSurface<S: Shape>(in shape: S) -> some View {
        if #available(iOS 26.0, macOS 26.0, *) {
            background(Color.themeGlassTint, in: shape)
                .glassEffect(.regular, in: shape)
        } else {
            #if os(iOS)
            background(Color.themeGlassTint, in: shape)
                .background(.regularMaterial, in: shape)
                .overlay { shape.stroke(Color.themeStrongBorder, lineWidth: 1) }
            #else
            background(Color.themeRaised.opacity(0.94), in: shape)
                .overlay { shape.stroke(Color.themeStrongBorder, lineWidth: 1) }
            #endif
        }
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
