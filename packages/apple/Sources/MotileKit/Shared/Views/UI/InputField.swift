import SwiftUI

/// How a field is set off from what it lies on.
enum InputVariant {
    /// A fill alone.
    case filled
    /// No background or height of its own, for a field that lies on glass.
    case bare
}

/// A line to type in, as tall as a button of its size, with a symbol before it when it has one
/// and a button that empties it when it is `clearable`, as Esc does while it has text. A `secure`
/// one shows dots for what is typed.
struct InputField: View {
    private let placeholder: String
    @Binding private var text: String
    private let icon: Symbol?
    private let size: ControlSize
    private let clearable: Bool
    private let monospaced: Bool
    private let secure: Bool
    private let variant: InputVariant
    private let focus: FocusState<Bool>.Binding?
    @FocusState private var ownFocus: Bool

    init(
        _ placeholder: String, text: Binding<String>, icon: Symbol? = nil, variant: InputVariant = .filled, size: ControlSize = .regular,
        clearable: Bool = false, monospaced: Bool = false, secure: Bool = false, focus: FocusState<Bool>.Binding? = nil
    ) {
        self.placeholder = placeholder
        _text = text
        self.icon = icon
        self.variant = variant
        self.size = size
        self.clearable = clearable
        self.monospaced = monospaced
        self.secure = secure
        self.focus = focus
    }

    /// A button inside the field is as far from its top and bottom as from its side.
    private var bare: Bool { variant == .bare }

    private var fill: Color {
        switch variant {
        case .filled: .themeInput
        case .bare: .clear
        }
    }

    private var clearInset: CGFloat { (size.height - ControlSize.small.height) / 2 }

    var body: some View {
        HStack(spacing: size.gap) {
            if let icon {
                Image(icon, size: size.smallSymbol)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .allowsHitTesting(false)
            }
            field
                .textFieldStyle(.plain)
                .font(.ui(size: size.textSize + 0.5, design: monospaced ? .monospaced : .default))
                .focused(focus ?? $ownFocus)
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
        .fieldFrame(size, fill: fill)
    }
}

extension InputField {
    @ViewBuilder fileprivate var field: some View {
        if secure {
            SecureField("", text: $text, prompt: .placeholder(placeholder))
        } else {
            TextField("", text: $text, prompt: .placeholder(placeholder))
        }
    }
}

extension Text {
    /// What a field shows while it is empty, in the one colour every field's placeholder has.
    static func placeholder(_ text: String) -> Text {
        Text(text).foregroundStyle(Color.themeMutedStrongerForeground)
    }
}

extension View {
    /// The fill of a field of the size: input, with no border, focused or not.
    func fieldFrame(_ size: ControlSize, fill: Color = .themeInput) -> some View {
        background(fill, in: RoundedRectangle(cornerRadius: size.radius, style: .continuous))
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
