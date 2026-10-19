import SwiftUI

/// How a field is set off from what it lies on.
enum InputVariant {
    /// A fill alone.
    case filled
    /// No background or height of its own, for a field that lies on glass, with its symbol in the
    /// foreground as the system's glass fields have.
    case bare
}

/// A line to type in, as tall as a button of its size, with a symbol before it when it has one
/// and a button that empties it when it is `clearable`, as Esc does while it has text. A `secure`
/// one shows dots for what is typed, a `numeric` one asks for the keypad of numbers.
struct InputField: View {
    private let placeholder: String
    @Binding private var text: String
    private let icon: Symbol?
    private let size: ControlSize
    private let clearable: Bool
    private let monospaced: Bool
    private let secure: Bool
    private let numeric: Bool
    private let variant: InputVariant
    private let focus: FocusState<Bool>.Binding?
    @FocusState private var ownFocus: Bool

    init(
        _ placeholder: String, text: Binding<String>, icon: Symbol? = nil, variant: InputVariant = .filled, size: ControlSize = .regular,
        clearable: Bool = false, monospaced: Bool = false, secure: Bool = false, numeric: Bool = false, focus: FocusState<Bool>.Binding? = nil
    ) {
        self.placeholder = placeholder
        _text = text
        self.icon = icon
        self.variant = variant
        self.size = size
        self.clearable = clearable
        self.monospaced = monospaced
        self.secure = secure
        self.numeric = numeric
        self.focus = focus
    }

    /// A button inside the field is as far from its top and bottom as from its side.
    private var bare: Bool { variant == .bare }

    private var clearInset: CGFloat { (size.height - ControlSize.small.height) / 2 }

    var body: some View {
        HStack(spacing: size.gap) {
            if let icon {
                Image(icon, size: size.smallSymbol)
                    .foregroundStyle(bare ? Color.themeForeground : Color.themeMutedStrongerForeground)
                    .allowsHitTesting(false)
            }
            field
                .textFieldStyle(.plain)
                .font(.ui(size: size.textSize + 0.5, design: monospaced ? .monospaced : .default))
                .focused(focus ?? $ownFocus)
                .numberKeypad(numeric)
                .onEscape(clearable && !text.isEmpty ? { text = "" } : nil)
            if clearable, !text.isEmpty {
                ActionButton(icon: .x, help: "Clear", size: .small) { text = "" }
                    .padding(.trailing, clearInset - size.padding + 2)
            }
        }
        .padding(.horizontal, size.padding - 2)
        .frame(height: bare ? nil : size.height)
        .frame(maxHeight: bare ? .infinity : nil)
        .background {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture { (focus ?? $ownFocus).wrappedValue = true }
                .textPointer()
        }
        .fieldFrame(size, filled: !bare)
    }
}

extension InputField {
    @ViewBuilder fileprivate var field: some View {
        if secure {
            SecureField("", text: $text).placeholder(placeholder, shown: text.isEmpty)
        } else {
            TextField("", text: $text).placeholder(placeholder, shown: text.isEmpty)
        }
    }
}

extension View {
    @ViewBuilder fileprivate func numberKeypad(_ shown: Bool) -> some View {
        #if os(iOS)
        if shown { keyboardType(.decimalPad) } else { self }
        #else
        self
        #endif
    }
}

extension View {
    /// What a field shows while it is empty, in the one colour every field's placeholder has. It is
    /// drawn behind the field, as the Mac's field ignores the colour of its prompt.
    func placeholder(_ text: String, shown: Bool) -> some View {
        background(alignment: .leading) {
            if shown {
                Text(text)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .lineLimit(1)
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
            }
        }
        .accessibilityLabel(text)
    }
}

/// The fill of a field of the size, with no border, focused or not. On a card, which is as dark
/// as input, it takes the card's accent.
private struct FieldFrame: ViewModifier {
    let size: ControlSize
    let filled: Bool
    @Environment(\.surface) private var surface

    private var fill: Color {
        guard filled else { return .clear }
        return surface == .card ? surface.color(.box) : .themeInput
    }

    func body(content: Content) -> some View {
        content.background(fill, in: RoundedRectangle(cornerRadius: size.radius, style: .continuous))
    }
}

extension View {
    func fieldFrame(_ size: ControlSize, filled: Bool = true) -> some View {
        modifier(FieldFrame(size: size, filled: filled))
    }
}

extension View {
    /// The cursor of text, over what starts typing when it is clicked. A control inside keeps the arrow.
    @ViewBuilder func textPointer(_ shown: Bool = true) -> some View {
        #if os(macOS)
        if #available(macOS 15.0, *) {
            pointerStyle(shown ? .horizontalText : .default)
        } else {
            self
        }
        #else
        self
        #endif
    }
}
