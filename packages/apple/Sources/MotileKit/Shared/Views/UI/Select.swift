import SwiftUI

/// One of a list, picked from a button as wide as it is given: the chosen one, or `placeholder`
/// in a field's placeholder colour, at its start and its chevron at its end. On the Mac its
/// options drop as a menu with a check by the chosen one; on iOS they are a sheet.
struct Select: View {
    private let options: [String]
    private let selected: Int?
    private let placeholder: String
    private let size: ControlSize
    private let onSelect: (Int) -> Void
    @State private var offered: [Choice] = []
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.surface) private var surface

    init(_ options: [String], selected: Int?, placeholder: String = "Choose", size: ControlSize = .regular, onSelect: @escaping (Int) -> Void) {
        self.options = options
        self.selected = selected
        self.placeholder = placeholder
        self.size = size
        self.onSelect = onSelect
    }

    private var chosen: String? {
        guard let selected, options.indices.contains(selected) else { return nil }
        return options[selected]
    }

    var body: some View {
        menu
            .opacity(.disabled, when: !enabled)
            .accessibilityLabel(chosen ?? placeholder)
    }

    private var label: some View {
        HStack(spacing: size.gap) {
            Text(chosen ?? placeholder)
                .font(size.font)
                .foregroundStyle(chosen == nil ? Color.themeMutedStrongerForeground : Color.themeForeground)
                .lineLimit(1)
            Spacer(minLength: 0)
            Image(.chevronDown, size: size.chevron, trimmed: true)
                .foregroundStyle(Color.themeMutedStrongerForeground)
        }
        .padding(.leading, size.padding)
        .padding(.trailing, size.padding - size.chevronOutset)
        .frame(height: size.height)
        .contentShape(Rectangle())
    }

    #if os(macOS)
    private var menu: some View {
        var look = ControlLook(variant: .secondary, size: size)
        look.surface = surface
        look.lit = enabled && hovering
        return Menu {
            Picker("", selection: Binding { selected } set: { if let index = $0 { onSelect(index) } }) {
                ForEach(options.indices, id: \.self) { index in
                    Text(options[index]).tag(Optional(index))
                }
            }
            .pickerStyle(.inline)
            .labelsHidden()
        } label: {
            label
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .background { look.background }
        .onHover { hovering = $0 }
    }
    #else
    private var menu: some View {
        Button {
            Choice.offer(options.indices.map { index in Choice(options[index], chosen: index == selected) { onSelect(index) } }, in: $offered)
        } label: {
            label
        }
        .buttonStyle(ControlButtonStyle(look: ControlLook(variant: .secondary, size: size)))
        .choices($offered)
    }
    #endif
}
