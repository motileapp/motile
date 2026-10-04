import SwiftUI

/// Lights up what the pointer is over, what is selected and what is `lit`. The light is drawn `inset` from the
/// view's edges: that margin looks empty but is the view's, so neighbours leave no gap to miss.
/// What is `faded` is in the secondary color until it lights up.
private struct HoverHighlight: ViewModifier {
    let radius: CGFloat
    let selected: Bool
    let lit: Bool
    let inset: EdgeInsets
    let color: Color
    let faded: Bool
    @State private var hovering = false
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
        if selected { return Color.themeSelected }
        return hovering || lit ? color : Color.clear
    }

    private var text: Color {
        guard enabled, selected || hovering || lit else { return Color.themeSecondary }
        return Color.themeText
    }
}

extension View {
    func hoverHighlight(
        radius: CGFloat = 7, selected: Bool = false, lit: Bool = false, inset: EdgeInsets = EdgeInsets(), color: Color = .themeHover,
        faded: Bool = false
    ) -> some View {
        modifier(HoverHighlight(radius: radius, selected: selected, lit: lit, inset: inset, color: color, faded: faded))
    }
}

/// What a button looks like under the pointer and under a finger: the same light. A button that
/// is selected, or `lit` as the one the arrow keys are on, has it without either.
struct HighlightButtonStyle: ButtonStyle {
    var radius: CGFloat = 7
    var selected = false
    var lit = false
    var inset = EdgeInsets()
    var color = Color.themeHover
    var faded = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .hoverHighlight(radius: radius, selected: selected, lit: lit || configuration.isPressed, inset: inset, color: color, faded: faded)
    }
}

/// A button whose own look says what it does, like a picture: it only dims under a finger.
struct DimButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.opacity(configuration.isPressed ? 0.7 : 1)
    }
}

extension ButtonStyle where Self == HighlightButtonStyle {
    static func highlight(
        radius: CGFloat = 7, selected: Bool = false, lit: Bool = false, inset: EdgeInsets = EdgeInsets(), color: Color = .themeHover,
        faded: Bool = false
    ) -> HighlightButtonStyle {
        HighlightButtonStyle(radius: radius, selected: selected, lit: lit, inset: inset, color: color, faded: faded)
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

/// A button that is only a symbol, with room around it to hit and a background under the pointer.
/// The inset is more room to hit, outside the background. Under a finger it takes presses as far
/// around it as a finger needs, without taking that room in the layout.
struct IconOnlyButton: View {
    let symbol: Symbol
    let help: String
    var size: CGFloat = scaled(26)
    var symbolSize: CGFloat = 13
    var radius: CGFloat = 6
    var inset = EdgeInsets()
    var faded = false
    let action: () -> Void

    var body: some View {
        let reach = max(0, (Platform.minimumPress - size) / 2)
        let around = EdgeInsets(top: inset.top + reach, leading: inset.leading + reach, bottom: inset.bottom + reach, trailing: inset.trailing + reach)
        Button(action: action) {
            Image(symbol, size: symbolSize)
                .frame(width: size, height: size)
                .padding(around)
                .contentShape(Rectangle())
        }
        .buttonStyle(.highlight(radius: radius, inset: around, faded: faded))
        .padding(-reach)
        .help(help)
        .accessibilityLabel(help)
    }
}

/// A button that is only its text, in the color of a link. It takes the room of the text alone:
/// the background under the pointer and the room to hit reach past it, into the space around.
struct LinkButton: View {
    static let height: CGFloat = 30
    static let padding: CGFloat = 8

    let title: String
    let action: () -> Void

    init(_ title: String, action: @escaping () -> Void) {
        self.title = title
        self.action = action
    }

    var body: some View {
        text
            .hidden()
            .overlay {
                Button(action: action) {
                    text
                        .foregroundStyle(Color.themeLink)
                        .padding(.horizontal, Self.padding)
                        .frame(minWidth: Self.height, minHeight: Self.height)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.highlight(radius: 6, color: .themeLinkHover))
                .fixedSize()
            }
    }

    private var text: some View {
        Text(title)
            .font(.ui(size: 12))
            .lineLimit(1)
    }
}

/// A button in the window's toolbar. It lights up as a rounded rectangle, like a row of the
/// sidebar, and the buttons touch: the space seen between them is theirs.
struct ToolbarButton: View {
    static let margin: CGFloat = 2
    static let width: CGFloat = 28 + 2 * margin

    let symbol: Symbol
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
