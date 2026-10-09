import SwiftUI

/// A short row of choices of which one is on, in a track with the border
/// around it. The choices touch each other and the track's edge, so no click falls between them;
/// their light is drawn `inset` from their edges, as the sidebar's rows are.
struct Segmented<Value: Hashable>: View {
    /// How far a choice's light stands from the track's border, and from the next one's.
    static var inset: CGFloat { 4 }
    static var border: CGFloat { 1 }

    private let options: [(title: String, value: Value)]
    @Binding private var selection: Value
    private let size: ControlSize
    private let fills: Bool

    /// With `fills`, the choices share the whole width they are given.
    init(_ options: [(title: String, value: Value)], selection: Binding<Value>, size: ControlSize = .regular, fills: Bool = false) {
        self.options = options
        _selection = selection
        self.size = size
        self.fills = fills
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: size.radius + Self.inset + Self.border, style: .continuous)
        HStack(spacing: 0) {
            ForEach(options.indices, id: \.self) { index in
                let option = options[index]
                Segment(
                    title: option.title, selected: option.value == selection, size: size, fills: fills,
                    margin: margin(at: index)
                ) {
                    selection = option.value
                }
            }
        }
        .layered(in: shape)
        .overlay { shape.strokeBorder(Color.themeBorderCard, lineWidth: Self.border) }
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
    let selected: Bool
    let size: ControlSize
    let fills: Bool
    let margin: EdgeInsets
    let action: () -> Void

    var body: some View {
        Text(title)
            .font(size.font)
            .lineLimit(1)
            .padding(.horizontal, size.padding)
            .frame(maxWidth: fills ? .infinity : nil)
            .frame(height: size.height)
            .padding(margin)
            .button(.highlight(radius: size.radius, selected: selected, inset: margin, faded: true), action: action)
    }
}
