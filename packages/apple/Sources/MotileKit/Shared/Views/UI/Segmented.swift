import SwiftUI

/// A short row of choices of which one is on, in a track on the background with the border
/// around it. The choices touch each other and the track's edge, so no click falls between them;
/// their light is drawn `inset` from their edges, as the sidebar's rows are.
struct Segmented<Value: Hashable>: View {
    /// How far a choice's light stands from the track's border, and from the next one's.
    static var inset: CGFloat { 2 }
    static var border: CGFloat { 1 }
    /// The track of a compact one, which lies in a dropdown, is as round as the dropdown's
    /// corners, and its choices' lights round to follow it.
    static var compactRadius: CGFloat { Radius.md }

    private let options: [(title: String, symbol: Symbol?, value: Value)]
    @Binding private var selection: Value
    private let size: ControlSize
    private let fills: Bool
    private var compact = false
    private var lit: Value?
    private var hovered: Binding<Value?>?
    @Environment(\.isEnabled) private var enabled

    /// With `fills`, the choices share the whole width they are given.
    init(_ options: [(title: String, value: Value)], selection: Binding<Value>, size: ControlSize = .regular, fills: Bool = false) {
        self.options = options.map { ($0.title, nil, $0.value) }
        _selection = selection
        self.size = size
        self.fills = fills
    }

    /// Choices that are only their symbols, each named when the pointer rests on it. `hovered`
    /// says which one the pointer is on, and `lit` is the one the arrow keys are on.
    init(
        symbols options: [(title: String, symbol: Symbol, value: Value)], selection: Binding<Value>, size: ControlSize = .regular,
        fills: Bool = false, compact: Bool = false, lit: Value? = nil, hovered: Binding<Value?>? = nil
    ) {
        self.options = options.map { ($0.title, $0.symbol, $0.value) }
        _selection = selection
        self.size = size
        self.fills = fills
        self.compact = compact
        self.lit = lit
        self.hovered = hovered
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: compact ? Self.compactRadius : size.radius + Self.inset + Self.border, style: .continuous)
        HStack(spacing: 0) {
            ForEach(options.indices, id: \.self) { index in
                let option = options[index]
                Segment(
                    title: option.title, symbol: option.symbol, selected: option.value == selection, lit: option.value == lit, size: size,
                    radius: compact ? Self.compactRadius - Self.inset - Self.border : size.radius, fills: fills, margin: margin(at: index)
                ) {
                    selection = option.value
                }
                .onHover { inside in
                    guard let hovered else { return }
                    if inside {
                        hovered.wrappedValue = option.value
                    } else if hovered.wrappedValue == option.value {
                        hovered.wrappedValue = nil
                    }
                }
            }
        }
        .environment(\.surface, .background)
        .background(Color.themeSegment, in: shape)
        .overlay { shape.strokeBorder(Color.themeBorder, lineWidth: Self.border) }
        .opacity(.disabled, when: !enabled)
        .animation(.easeOut(duration: 0.12), value: selection)
    }

    private func margin(at index: Int) -> EdgeInsets {
        let half = Self.inset / 2
        let edge = Self.inset + Self.border
        return EdgeInsets(
            top: edge, leading: index == 0 ? edge : half, bottom: edge,
            trailing: index == options.count - 1 ? edge : half)
    }
}

private struct Segment: View {
    let title: String
    let symbol: Symbol?
    let selected: Bool
    let lit: Bool
    let size: ControlSize
    let radius: CGFloat
    let fills: Bool
    let margin: EdgeInsets
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            words
                .padding(.horizontal, size.padding)
                .frame(maxWidth: fills ? .infinity : nil)
                .frame(height: size.height)
                .padding(margin)
        }
        .buttonStyle(SegmentStyle(selected: selected, lit: lit, radius: radius, margin: margin))
        .help(symbol == nil ? "" : title)
        .accessibilityLabel(title)
    }

    @ViewBuilder private var words: some View {
        if let symbol {
            Image(symbol, size: size.symbol)
        } else {
            Text(title)
                .font(size.font)
                .lineLimit(1)
        }
    }
}

/// The chosen segment stands on the track in its own colour with a small shadow; another lies
/// on the track's accent while the pointer is over it, a press holds it or the arrow keys are on
/// it. Its words are in the foreground then, and muted else.
private struct SegmentStyle: ButtonStyle {
    let selected: Bool
    let lit: Bool
    let radius: CGFloat
    let margin: EdgeInsets

    func makeBody(configuration: Configuration) -> some View {
        SegmentBody(label: configuration.label, pressed: configuration.isPressed, selected: selected, lit: lit, radius: radius, margin: margin)
            .arrowPointer()
    }
}

private struct SegmentBody<Label: View>: View {
    let label: Label
    let pressed: Bool
    let selected: Bool
    let lit: Bool
    let radius: CGFloat
    let margin: EdgeInsets
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        let up = enabled && (hovering || pressed || lit)
        label
            .foregroundStyle(enabled ? selected || up ? Color.themeForeground : Color.themeMutedForeground : Color.themeMutedStrongerForeground)
            .background {
                Group {
                    if selected {
                        shape.fill(Color.themeSegmentSelected).shadow(.sm)
                    } else if up {
                        shape.fill(Color.themeSegmentAccent)
                    }
                }
                .padding(margin)
            }
            .contentShape(Rectangle())
            .onHover { hovering = $0 }
    }
}
