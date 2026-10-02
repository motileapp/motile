import SwiftUI

/// Lights up what the pointer is over, and what is selected. The light is drawn `inset` from the
/// view's edges: that margin looks empty but is the view's, so neighbours leave no gap to miss.
private struct HoverHighlight: ViewModifier {
    let radius: CGFloat
    let selected: Bool
    let inset: EdgeInsets
    @State private var hovering = false

    func body(content: Content) -> some View {
        content
            .background {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(fill)
                    .padding(inset)
            }
            .onHover { hovering = $0 }
    }

    private var fill: Color {
        if selected { return Color.themeSelected }
        return hovering ? Color.themeHover : Color.clear
    }
}

extension View {
    func hoverHighlight(radius: CGFloat = 7, selected: Bool = false, inset: EdgeInsets = EdgeInsets()) -> some View {
        modifier(HoverHighlight(radius: radius, selected: selected, inset: inset))
    }
}

/// A button that is only a symbol, with room around it to hit and a background under the pointer.
/// The inset is more room to hit, outside the background.
struct IconOnlyButton: View {
    let symbol: String
    let help: String
    var size: CGFloat = 26
    var symbolSize: CGFloat = 13
    var radius: CGFloat = 6
    var inset = EdgeInsets()
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: symbolSize, weight: .medium))
                .frame(width: size, height: size)
                .padding(inset)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .hoverHighlight(radius: radius, inset: inset)
        .help(help)
    }
}

/// A button in the window's toolbar. It lights up as a rounded rectangle, like a row of the
/// sidebar, and the buttons touch: the space seen between them is theirs.
struct ToolbarButton: View {
    static let margin: CGFloat = 2
    static let width: CGFloat = 28 + 2 * margin

    let symbol: String
    let help: String
    let action: () -> Void

    var body: some View {
        IconOnlyButton(
            symbol: symbol,
            help: help,
            size: Self.width - 2 * Self.margin,
            symbolSize: 15,
            radius: 7,
            inset: EdgeInsets(top: Self.margin, leading: Self.margin, bottom: Self.margin, trailing: Self.margin),
            action: action
        )
    }
}
