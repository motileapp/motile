#if os(iOS)
import SwiftUI

/// A symbol's button that floats on glass over a picture: on a glass circle of its own, or beside
/// others on one glass capsule that `glassButton(in:)` draws around them.
struct GlassButtonStyle: ButtonStyle {
    static let side: CGFloat = 46

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(width: Self.side, height: Self.side)
            .contentShape(Rectangle())
    }
}

extension ButtonStyle where Self == GlassButtonStyle {
    static var glassSymbol: GlassButtonStyle { GlassButtonStyle() }
}
#endif
