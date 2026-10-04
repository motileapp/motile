#if os(iOS)
import SwiftUI

extension View {
    /// The surface of a button that floats over the client: Liquid Glass where the system has it,
    /// and the system's material with a hairline around it before that.
    @ViewBuilder
    func glassButton<S: Shape>(in shape: S) -> some View {
        if #available(iOS 26.0, *) {
            glassEffect(.regular.interactive(), in: shape)
        } else {
            background(.regularMaterial, in: shape)
                .overlay { shape.stroke(Color.themeStrongBorder, lineWidth: 1) }
        }
    }
}
#endif
