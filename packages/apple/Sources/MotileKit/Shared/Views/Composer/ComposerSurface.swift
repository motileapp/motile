import SwiftUI

extension View {
    /// The surface of the composer and its strips. On the Mac it is the composer's colour with a
    /// hairline around it, and `shadow` is cast by the fill alone: on the whole box, the text
    /// view inside splits it into layers that shade each other. On iOS it is Liquid Glass under
    /// a tint where the system has it, which draws its own edge, and the system's material with
    /// a hairline before that.
    func composerSurface<S: Shape>(in shape: S, shadow: ShadowSize? = nil) -> some View {
        composerFill(in: shape, shadow: shadow)
            .composerSilhouette(shape)
            .environment(\.surface, .background)
    }

    /// The wide shadow the composer and its strips cast together, from one silhouette of their
    /// surfaces behind them all.
    func composerOutlineShadow() -> some View {
        #if os(macOS)
        backgroundPreferenceValue(ComposerSilhouette.self) { parts in
            GeometryReader { proxy in
                ZStack {
                    ForEach(parts.indices, id: \.self) { index in
                        parts[index].path(proxy[parts[index].bounds]).fill(Color.themeComposer)
                    }
                }
                .shadow(.lg)
            }
        }
        #else
        self
        #endif
    }

    private func composerSilhouette<S: Shape>(_ shape: S) -> some View {
        #if os(macOS)
        anchorPreference(key: ComposerSilhouette.self, value: .bounds) { [ComposerSilhouette.Part(bounds: $0, path: shape.path(in:))] }
        #else
        self
        #endif
    }

    @ViewBuilder
    private func composerFill<S: Shape>(in shape: S, shadow: ShadowSize?) -> some View {
        #if os(macOS)
        background {
            if let shadow {
                shape.fill(Color.themeComposer).shadow(shadow)
            } else {
                shape.fill(Color.themeComposer)
            }
        }
        .overlay { shape.stroke(Color.themeBorderComposer, lineWidth: 1) }
        #else
        if #available(iOS 26.0, *) {
            background(Color.themeComposer.at(.inputGlassTint), in: shape)
                .glassEffect(.regular, in: shape)
        } else {
            background(Color.themeComposer.at(.inputGlassTint), in: shape)
                .background(.regularMaterial, in: shape)
                .overlay { shape.stroke(Color.themeBorderComposer, lineWidth: 1) }
        }
        #endif
    }
}

#if os(macOS)
/// The surfaces of the composer and its strips, where they are and their outlines.
struct ComposerSilhouette: PreferenceKey {
    struct Part {
        let bounds: Anchor<CGRect>
        let path: (CGRect) -> Path
    }

    static let defaultValue: [Part] = []

    static func reduce(value: inout [Part], nextValue: () -> [Part]) {
        value += nextValue()
    }
}
#endif

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
