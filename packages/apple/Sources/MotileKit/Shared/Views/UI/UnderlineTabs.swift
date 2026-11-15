import SwiftUI

/// Tabs as words on a line across the whole width, the one on underlined. The tabs touch, so no
/// click falls between them; their lights and lines are drawn `gap` apart, and one lights as a
/// ghost button does above the line.
struct UnderlineTabs<Value: Hashable>: View {
    static var underline: CGFloat { 2 }
    static var gap: CGFloat { 4 }
    /// Between a tab's words and its sides.
    static var padding: CGFloat { 16 }
    /// Between a tab's light and the line under it.
    static var lightGap: CGFloat { 4 }

    private let options: [(title: String, value: Value)]
    @Binding private var selection: Value
    private let size: ControlSize

    init(_ options: [(title: String, value: Value)], selection: Binding<Value>, size: ControlSize = .regular) {
        self.options = options
        _selection = selection
        self.size = size
    }

    var body: some View {
        HStack(spacing: 0) {
            ForEach(options.indices, id: \.self) { index in
                let option = options[index]
                Button { selection = option.value } label: {
                    Text(option.title)
                        .font(size.font)
                        .lineLimit(1)
                        .padding(.horizontal, Self.padding)
                        .frame(height: size.height)
                }
                .buttonStyle(UnderlineTabStyle(selected: option.value == selection, size: size, margin: margin(at: index)))
                .accessibilityAddTraits(option.value == selection ? .isSelected : [])
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(alignment: .bottom) {
            Rectangle().fill(Color.themeBorder).frame(height: 1)
        }
    }

    /// Half the gap on the sides that meet another tab.
    private func margin(at index: Int) -> EdgeInsets {
        EdgeInsets(top: 0, leading: index == 0 ? 0 : Self.gap / 2, bottom: 0, trailing: index == options.count - 1 ? 0 : Self.gap / 2)
    }
}

/// The tab that is on has its words in the foreground and the line under it; another lights
/// while the pointer is over it or a press holds it.
private struct UnderlineTabStyle: ButtonStyle {
    let selected: Bool
    let size: ControlSize
    let margin: EdgeInsets

    func makeBody(configuration: Configuration) -> some View {
        UnderlineTabBody(label: configuration.label, pressed: configuration.isPressed, selected: selected, size: size, margin: margin)
    }
}

private struct UnderlineTabBody<Label: View>: View {
    let label: Label
    let pressed: Bool
    let selected: Bool
    let size: ControlSize
    let margin: EdgeInsets
    @State private var hovering = false
    @Environment(\.surface) private var surface

    var body: some View {
        let lit = hovering || pressed
        VStack(spacing: UnderlineTabs<Int>.lightGap) {
            label
                .padding(margin)
                .foregroundStyle(selected || lit ? Color.themeForeground : Color.themeMutedForeground)
                .background {
                    if lit {
                        RoundedRectangle(cornerRadius: size.radius, style: .continuous)
                            .fill(surface.color(.control))
                            .padding(margin)
                    }
                }
            Capsule()
                .fill(selected ? Color.themeForeground : .clear)
                .frame(height: UnderlineTabs<Int>.underline)
                .padding(margin)
        }
        .fixedSize(horizontal: true, vertical: false)
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
    }
}
