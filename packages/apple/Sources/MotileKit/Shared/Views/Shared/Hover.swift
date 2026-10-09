import SwiftUI

/// Lights up what the pointer is over, what is selected and what is `lit`. The light is drawn `inset` from the
/// view's edges: that margin looks empty but is the view's, so neighbours leave no gap to miss.
/// What is `faded` is in the muted colour until it lights up. A view that follows the pointer
/// itself passes `hovered`, so that the pointer is followed once.
private struct HoverHighlight: ViewModifier {
    let radius: CGFloat
    let selected: Bool
    let lit: Bool
    let inset: EdgeInsets
    let faded: Bool
    let hovered: Bool?
    @State private var tracked = false
    @Environment(\.surface) private var surface
    @Environment(\.isEnabled) private var enabled

    private var hovering: Bool { hovered ?? tracked }

    @ViewBuilder func body(content: Content) -> some View {
        let light = self.light
        let lighted = tinted(content)
            .background {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(light.map(surface.color) ?? Color.clear)
                    .padding(inset)
            }
            .environment(\.row, light)
        if hovered == nil {
            lighted.onHover { tracked = $0 }
        } else {
            lighted
        }
    }

    @ViewBuilder private func tinted(_ content: Content) -> some View {
        if faded {
            content.foregroundStyle(text)
        } else {
            content
        }
    }

    private var light: Surface.Layer? {
        let up = enabled && (hovering || lit)
        if selected { return up ? .rowSelectedLit : .rowSelected }
        return up ? .row : nil
    }

    private var text: Color {
        guard enabled else { return Color.themeMutedStrongerForeground }
        guard selected || hovering || lit else { return Color.themeMutedForeground }
        return Color.themeForeground
    }
}

extension View {
    func hoverHighlight(
        radius: CGFloat = Radius.md, selected: Bool = false, lit: Bool = false, inset: EdgeInsets = EdgeInsets(), faded: Bool = false,
        hovered: Bool? = nil
    ) -> some View {
        modifier(HoverHighlight(radius: radius, selected: selected, lit: lit, inset: inset, faded: faded, hovered: hovered))
    }
}

/// What a button looks like under the pointer and under a finger: the same light. A button that
/// is selected, or `lit` as the one the arrow keys are on, has it without either. All of the
/// label takes the click, so wherever it lights it can be pressed.
struct HighlightButtonStyle: ButtonStyle {
    var radius: CGFloat = Radius.md
    var selected = false
    var lit = false
    var inset = EdgeInsets()
    var faded = false
    var hovered: Bool?

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .hoverHighlight(
                radius: radius, selected: selected, lit: lit || configuration.isPressed, inset: inset, faded: faded, hovered: hovered
            )
    }
}

/// A button whose own look says what it does, like a picture: under a finger it is lit, at
/// opacity lit.
struct DimButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .opacity(.lit, when: configuration.isPressed)
    }
}

extension ButtonStyle where Self == HighlightButtonStyle {
    static func highlight(
        radius: CGFloat = Radius.md, selected: Bool = false, lit: Bool = false, inset: EdgeInsets = EdgeInsets(), faded: Bool = false,
        hovered: Bool? = nil
    ) -> HighlightButtonStyle {
        HighlightButtonStyle(radius: radius, selected: selected, lit: lit, inset: inset, faded: faded, hovered: hovered)
    }
}

extension View {
    /// Makes the view a button, with that light unless it has another style.
    func button(_ style: some ButtonStyle = .highlight(), action: @escaping () -> Void) -> some View {
        Button(action: action) { self }
            .buttonStyle(style)
    }
}

/// As tall as it is on the Mac, and where fingers press it at least as tall as a finger needs.
func pressable(_ height: CGFloat) -> CGFloat {
    max(scaled(height), Platform.minimumPress)
}
