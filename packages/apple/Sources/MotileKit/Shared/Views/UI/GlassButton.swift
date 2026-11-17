#if os(iOS)
import SwiftUI

/// A symbol's button that floats on glass over a picture: on a glass circle of its own, or beside
/// others on one glass capsule that `glassButton(in:)` draws around them.
struct GlassButtonStyle: ButtonStyle {
    static let side = floatingHeight

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(width: Self.side, height: Self.side)
            .contentShape(Rectangle())
    }
}

extension ButtonStyle where Self == GlassButtonStyle {
    static var glassSymbol: GlassButtonStyle { GlassButtonStyle() }
}

/// Reads a page again, in the bar of a sheet that a swipe down closes: its symbol turns while it
/// reads.
struct RefreshGlassButton: View {
    let pending: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            TurningSymbol(symbol: .refreshCw, size: .lg, turning: pending)
        }
        .buttonStyle(.glassSymbol)
        .disabled(pending)
        .accessibilityLabel("Refresh")
    }
}

extension View {
    /// A menu's place among glass buttons, as large as theirs.
    func glassSymbolFrame() -> some View {
        frame(width: GlassButtonStyle.side, height: GlassButtonStyle.side)
            .contentShape(Rectangle())
    }
}
#endif
