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
                .overlay { shape.stroke(Color.themeBorderSecondary, lineWidth: 1) }
        }
    }
}

/// What closes a sheet: the system's round glass button with an x, or "Done" before iOS 26.
struct SheetCloseButton: View {
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        if #available(iOS 26, *) {
            Button(role: .close) { dismiss() }
        } else {
            Button("Done") { dismiss() }
        }
    }
}
#endif
