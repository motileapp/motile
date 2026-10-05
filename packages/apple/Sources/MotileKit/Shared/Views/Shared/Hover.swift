import SwiftUI

/// Lights up what the pointer is over, what is selected and what is `lit`. The light is drawn `inset` from the
/// view's edges: that margin looks empty but is the view's, so neighbours leave no gap to miss.
/// What is `faded` is in the secondary color until it lights up.
private struct HoverHighlight: ViewModifier {
    let radius: CGFloat
    let selected: Bool
    let lit: Bool
    let inset: EdgeInsets
    let faded: Bool
    @State private var hovering = false
    @Environment(\.surface) private var surface
    @Environment(\.isEnabled) private var enabled

    func body(content: Content) -> some View {
        tinted(content)
            .background {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(fill)
                    .padding(inset)
            }
            .onHover { hovering = $0 }
    }

    @ViewBuilder private func tinted(_ content: Content) -> some View {
        if faded {
            content.foregroundStyle(text)
        } else {
            content
        }
    }

    private var fill: Color {
        if selected { return surface.further.color }
        return enabled && (hovering || lit) ? surface.next.color : Color.clear
    }

    private var text: Color {
        guard enabled else { return Color.themeTertiary }
        guard selected || hovering || lit else { return Color.themeSecondary }
        return Color.themeText
    }
}

extension View {
    func hoverHighlight(
        radius: CGFloat = 7, selected: Bool = false, lit: Bool = false, inset: EdgeInsets = EdgeInsets(), faded: Bool = false
    ) -> some View {
        modifier(HoverHighlight(radius: radius, selected: selected, lit: lit, inset: inset, faded: faded))
    }
}

/// What a button looks like under the pointer and under a finger: the same light. A button that
/// is selected, or `lit` as the one the arrow keys are on, has it without either. All of the
/// label takes the click, so wherever it lights it can be pressed.
struct HighlightButtonStyle: ButtonStyle {
    var radius: CGFloat = 7
    var selected = false
    var lit = false
    var inset = EdgeInsets()
    var faded = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .hoverHighlight(radius: radius, selected: selected, lit: lit || configuration.isPressed, inset: inset, faded: faded)
    }
}

/// A button whose own look says what it does, like a picture: it only dims under a finger.
struct DimButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .contentShape(Rectangle())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}

extension ButtonStyle where Self == HighlightButtonStyle {
    static func highlight(
        radius: CGFloat = 7, selected: Bool = false, lit: Bool = false, inset: EdgeInsets = EdgeInsets(), faded: Bool = false
    ) -> HighlightButtonStyle {
        HighlightButtonStyle(radius: radius, selected: selected, lit: lit, inset: inset, faded: faded)
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
