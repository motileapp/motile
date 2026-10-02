import SwiftUI

/// Lights up what the pointer is over, and what is selected.
private struct HoverHighlight: ViewModifier {
    let radius: CGFloat
    let selected: Bool
    @State private var hovering = false

    func body(content: Content) -> some View {
        content
            .background(fill, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .onHover { hovering = $0 }
    }

    private var fill: Color {
        if selected { return Color.themeSelected }
        return hovering ? Color.themeHover : Color.clear
    }
}

extension View {
    func hoverHighlight(radius: CGFloat = 7, selected: Bool = false) -> some View {
        modifier(HoverHighlight(radius: radius, selected: selected))
    }
}

/// A button that is only a symbol, with room around it to hit and a background under the pointer.
struct IconOnlyButton: View {
    let symbol: String
    let help: String
    var size: CGFloat = 26
    var symbolSize: CGFloat = 13
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: symbolSize, weight: .medium))
                .frame(width: size, height: size)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .hoverHighlight(radius: 6)
        .help(help)
    }
}
