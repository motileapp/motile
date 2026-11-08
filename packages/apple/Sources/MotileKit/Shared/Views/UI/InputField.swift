import SwiftUI

/// How a field is set off from what it lies on.
enum InputVariant {
    /// A fill alone.
    case filled
    /// No background or height of its own, for a field that lies on glass, with its symbol in the
    /// foreground as the system's glass fields have.
    case bare
    /// A bare field that reaches the end of the capsule it lies on, whose clear button is a circle
    /// in that end, as far from its side as from its top and bottom.
    case capsule
}

/// A line to type in, as tall as a button of its size, with a symbol before it when it has one
/// and a button that empties it when it is `clearable`, as Esc does while it has text. A `secure`
/// one shows dots for what is typed, a `numeric` one asks for the keypad of numbers. On the Mac
/// its line is a `LineField`, so Return does what `onReturn` says.
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
    private var bare: Bool { variant != .filled }

    private var clearSize: ControlSize { size == .regular ? .inset : .small }
    private var clearInset: CGFloat { (size.height - clearSize.height) / 2 }

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
                clearButton
            }
        }
        .padding(.leading, size.padding - 2)
        .padding(.trailing, clearsInEnd ? 0 : size.padding - 2)
        .frame(height: bare ? nil : size.height)
        .frame(maxHeight: bare ? .infinity : nil)
        .background {
            TypingArea { (focus ?? $ownFocus).wrappedValue = true }
        }
        .fieldFrame(size, filled: !bare)
    }
}

extension InputField {
    private var clearsInEnd: Bool { variant == .capsule && clearable && !text.isEmpty }

    @ViewBuilder fileprivate var clearButton: some View {
        if variant == .capsule {
            Color.clear
                .aspectRatio(1, contentMode: .fit)
                .overlay { ActionButton(icon: .x, help: "Clear", symbolSize: ControlSize.regular.symbol, round: true) { text = "" } }
        } else {
            ActionButton(icon: .x, help: "Clear", size: clearSize) { text = "" }
                .padding(.trailing, clearInset - size.padding + 2)
        }
    }

    @ViewBuilder fileprivate var field: some View {
        #if os(macOS)
        let font: NSFont = monospaced ? .uiMono(size.textSize + 0.5) : .ui(size.textSize + 0.5)
        LineField(text: $text, font: font, secure: secure).placeholder(placeholder, shown: text.isEmpty)
        #else
        if secure {
            SecureField("", text: $text).placeholder(placeholder, shown: text.isEmpty)
        } else {
            TextField("", text: $text).placeholder(placeholder, shown: text.isEmpty)
        }
        #endif
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

/// Room that starts typing when it is pressed, under the cursor of text where nothing lies over it.
struct TypingArea: View {
    var press: (() -> Void)?

    var body: some View {
        #if os(macOS)
        TypingAreaMac(press: press)
        #else
        if let press {
            Color.clear
                .contentShape(Rectangle())
                .onTapGesture(perform: press)
        }
        #endif
    }
}
