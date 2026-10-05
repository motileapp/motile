import SwiftUI

/// A line to type in, as tall as a button of its size, with a symbol before it when it has one
/// and a button that empties it when it is `clearable`. A `bare` one has no background or height
/// of its own, for a field that lies on glass.
struct InputField: View {
    private let placeholder: String
    @Binding private var text: String
    private let icon: Symbol?
    private let size: ControlSize
    private let clearable: Bool
    private let monospaced: Bool
    private let bare: Bool
    private let focus: FocusState<Bool>.Binding?
    @FocusState private var ownFocus: Bool

    init(
        _ placeholder: String, text: Binding<String>, icon: Symbol? = nil, size: ControlSize = .regular, clearable: Bool = false,
        monospaced: Bool = false, bare: Bool = false, focus: FocusState<Bool>.Binding? = nil
    ) {
        self.placeholder = placeholder
        _text = text
        self.icon = icon
        self.size = size
        self.clearable = clearable
        self.monospaced = monospaced
        self.bare = bare
        self.focus = focus
    }

    /// A button inside the field is as far from its top and bottom as from its side.
    private var clearInset: CGFloat { (size.height - ControlSize.small.height) / 2 }

    var body: some View {
        HStack(spacing: size.gap) {
            if let icon {
                Image(icon, size: size.symbol - 2)
                    .foregroundStyle(Color.themeTertiary)
            }
            TextField(placeholder, text: $text)
                .textFieldStyle(.plain)
                .font(.ui(size: size.textSize + 0.5, design: monospaced ? .monospaced : .default))
                .focused(focus ?? $ownFocus)
            if clearable, !text.isEmpty {
                ActionButton(icon: .x, help: "Clear", size: .small) { text = "" }
                    .padding(.trailing, clearInset - size.padding + 2)
            }
        }
        .padding(.horizontal, size.padding - 2)
        .frame(height: bare ? nil : size.height)
        .frame(maxHeight: bare ? .infinity : nil)
        .background(bare ? Color.clear : Color.themeField, in: RoundedRectangle(cornerRadius: size.radius, style: .continuous))
        .overlay {
            if !bare {
                RoundedRectangle(cornerRadius: size.radius, style: .continuous).strokeBorder(Color.themeStrongBorder, lineWidth: 1)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { (focus ?? $ownFocus).wrappedValue = true }
    }
}
