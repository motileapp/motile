import SwiftUI

/// A short row of choices of which one is on, each a control of its size, in a track that is a
/// card around them: the one that is on stands on the border colour, the others are in the
/// secondary colour until the pointer is over them.
struct Segmented<Value: Hashable>: View {
    /// How far the choices stand from the track's edge, which has the border in it.
    static var inset: CGFloat { 4 }

    private let options: [(title: String, value: Value)]
    @Binding private var selection: Value
    private let size: ControlSize
    @Environment(\.isEnabled) private var enabled

    init(_ options: [(title: String, value: Value)], selection: Binding<Value>, size: ControlSize = .regular) {
        self.options = options
        _selection = selection
        self.size = size
    }

    var body: some View {
        HStack(spacing: Self.inset) {
            ForEach(options.indices, id: \.self) { index in
                let option = options[index]
                Segment(title: option.title, selected: option.value == selection, size: size, fill: Color.themeBorder) {
                    selection = option.value
                }
            }
        }
        .padding(Self.inset)
        .card(radius: size.radius + Self.inset)
        .opacity(enabled ? 1 : 0.45)
        .animation(.easeOut(duration: 0.12), value: selection)
    }
}

private struct Segment: View {
    let title: String
    let selected: Bool
    let size: ControlSize
    let fill: Color
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(size.font)
                .lineLimit(1)
                .foregroundStyle(selected || hovering ? Color.themeText : Color.themeSecondary)
                .padding(.horizontal, size.padding)
                .frame(height: size.height)
                .background(fill.opacity(selected ? 1 : hovering ? 0.5 : 0), in: RoundedRectangle(cornerRadius: size.radius, style: .continuous))
                .contentShape(Rectangle())
        }
        .buttonStyle(DimButtonStyle())
        .onHover { hovering = $0 }
    }
}
