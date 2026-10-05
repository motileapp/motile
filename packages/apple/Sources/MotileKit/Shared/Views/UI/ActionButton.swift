import SwiftUI

/// What a button is for, which is how it looks.
enum ButtonVariant {
    /// The one thing the view calls for.
    case primary
    /// The others beside it.
    case secondary
    /// What can't be taken back.
    case danger
    /// Nothing until the pointer is over it: the buttons of bars and rows.
    case ghost
    /// In the colour of a link.
    case link
    /// A link's colour on a wash of it: a choice that is on.
    case accent
}

/// What a button shows before its words: a symbol, or a picture of its own like a logo.
enum ControlIcon {
    case symbol(Symbol)
    case picture(AnyView)
}

/// The colours and the shape of a button or a menu. Pressed looks as under the pointer does.
struct ControlLook {
    let variant: ButtonVariant
    let size: ControlSize
    var selected = false
    var lit = false
    /// Round ends, for the composer's buttons.
    var round = false
    /// The sides another control touches, which stay square.
    var joined: HorizontalEdge.Set = []
    /// A colour that says something, like a pull request's state, in place of the quiet ones.
    var tint: Color?

    var foreground: Color {
        if let tint, variant == .ghost || variant == .secondary { return tint }
        switch variant {
        case .secondary: return .themeText
        case .primary, .danger: return .white
        case .ghost: return selected || lit ? .themeText : .themeSecondary
        case .link, .accent: return .themeLink
        }
    }

    private var fill: Color {
        switch variant {
        case .primary: .themePrimary
        case .secondary: selected || lit ? .themeSelected : .themeHover
        case .danger: .themeDangerFill
        case .ghost: selected ? .themeSelected : lit ? .themeHover : .clear
        case .link: lit ? .themeLinkHover : .clear
        case .accent: .themeLink.opacity(lit ? 0.22 : 0.14)
        }
    }

    private var shape: UnevenRoundedRectangle {
        let radius = round ? size.height / 2 : size.radius
        let leading = joined.contains(.leading) ? 0 : radius
        let trailing = joined.contains(.trailing) ? 0 : radius
        return UnevenRoundedRectangle(
            topLeadingRadius: leading, bottomLeadingRadius: leading, bottomTrailingRadius: trailing, topTrailingRadius: trailing,
            style: .continuous)
    }

    var background: some View {
        shape
            .fill(fill)
            .overlay {
                if variant == .primary || variant == .danger, lit { shape.fill(Color.white.opacity(0.12)) }
            }
    }
}

/// What a button or a menu says: a symbol, words, or both. While `pending` the spinner stands
/// where the symbol is, or over the words when there is none, and the width stays.
struct ControlLabel: View {
    let title: String?
    let icon: ControlIcon?
    let size: ControlSize
    var chevron = false
    var pending = false
    var fills = false

    private var wordless: Bool { title == nil && !chevron }

    var body: some View {
        HStack(spacing: size.gap) {
            if let icon { mark(icon) }
            if let title {
                Text(title)
                    .font(size.font)
                    .lineLimit(1)
                    .opacity(pending && icon == nil ? 0 : 1)
            }
            if chevron {
                Image(.chevronDown, size: size.textSize, trimmed: true)
                    .opacity(0.6)
            }
        }
        .frame(maxWidth: fills ? .infinity : nil)
        .padding(.horizontal, wordless ? 0 : size.padding)
        .frame(minWidth: size.height)
        .frame(height: size.height)
        .overlay {
            if pending, icon == nil { Spinner(size: size.symbol) }
        }
    }

    @ViewBuilder private func mark(_ icon: ControlIcon) -> some View {
        if pending {
            Spinner(size: size.symbol)
        } else {
            switch icon {
            case .symbol(let symbol): Image(symbol, size: size.symbol)
            case .picture(let picture): picture.frame(width: size.symbolSide, height: size.symbolSide)
            }
        }
    }
}

/// How far around a control a finger still presses it, and the room that is the control's own
/// outside its background.
private struct ControlReach {
    let around: EdgeInsets
    let outset: EdgeInsets

    init(size: ControlSize, wordless: Bool, margin: EdgeInsets) {
        let reach = max(0, (Platform.minimumPress - size.height) / 2)
        let sideways = wordless ? reach : 0
        around = EdgeInsets(
            top: margin.top + reach, leading: margin.leading + sideways, bottom: margin.bottom + reach, trailing: margin.trailing + sideways)
        outset = EdgeInsets(top: -reach, leading: -sideways, bottom: -reach, trailing: -sideways)
    }
}

struct ControlButtonStyle: ButtonStyle {
    let look: ControlLook
    var pending = false
    var margin = EdgeInsets()

    func makeBody(configuration: Configuration) -> some View {
        ControlBody(label: configuration.label, pressed: configuration.isPressed, look: look, pending: pending, margin: margin)
    }
}

private struct ControlBody<Label: View>: View {
    let label: Label
    let pressed: Bool
    let look: ControlLook
    let pending: Bool
    let margin: EdgeInsets
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        var look = self.look
        look.lit = enabled && !pending && (hovering || pressed)
        return label
            .foregroundStyle(look.foreground)
            .background { look.background.padding(margin) }
            .contentShape(Rectangle())
            .opacity(enabled || pending ? 1 : 0.45)
            .onHover { hovering = $0 }
    }
}

extension ButtonStyle where Self == ControlButtonStyle {
    /// The button's look for what the system makes a button of, like a share link. Its label is
    /// a `ControlLabel` of the same size.
    static func control(_ variant: ButtonVariant = .ghost, size: ControlSize = .regular) -> ControlButtonStyle {
        ControlButtonStyle(look: ControlLook(variant: variant, size: size))
    }
}

/// The button. It says what it does with words, a symbol or both, in one of three sizes. While
/// `pending` it shows the spinner and takes no presses. `margin` is room around it that is its
/// own, so that neighbours leave no gap to miss.
struct ActionButton: View {
    private let title: String?
    private let icon: ControlIcon?
    private let help: String?
    private let look: ControlLook
    /// It opens something to choose from, and shows a chevron for it.
    private var chevron = false
    private let pending: Bool
    private let fills: Bool
    private let margin: EdgeInsets
    private let action: () -> Void

    init(
        _ title: String, icon: Symbol? = nil, picture: AnyView? = nil, help: String? = nil, variant: ButtonVariant = .secondary,
        size: ControlSize = .regular, pending: Bool = false, selected: Bool = false, round: Bool = false, fills: Bool = false,
        opens: Bool = false, joined: HorizontalEdge.Set = [], tint: Color? = nil, margin: EdgeInsets = EdgeInsets(),
        action: @escaping () -> Void
    ) {
        chevron = opens
        self.title = title
        self.icon = icon.map(ControlIcon.symbol) ?? picture.map(ControlIcon.picture)
        self.help = help
        look = ControlLook(variant: variant, size: size, selected: selected, round: round, joined: joined, tint: tint)
        self.pending = pending
        self.fills = fills
        self.margin = margin
        self.action = action
    }

    /// A button that is only a symbol says what it does in `help`.
    init(
        icon: Symbol, help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular, pending: Bool = false,
        selected: Bool = false, round: Bool = false, joined: HorizontalEdge.Set = [], tint: Color? = nil,
        margin: EdgeInsets = EdgeInsets(), action: @escaping () -> Void
    ) {
        title = nil
        self.icon = .symbol(icon)
        self.help = help
        look = ControlLook(variant: variant, size: size, selected: selected, round: round, joined: joined, tint: tint)
        self.pending = pending
        fills = false
        self.margin = margin
        self.action = action
    }

    /// The half of a split button that opens what the other half doesn't do: only a chevron.
    static func chevron(
        help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular, joined: HorizontalEdge.Set = [],
        action: @escaping () -> Void
    ) -> ActionButton {
        var button = ActionButton("", help: help, variant: variant, size: size, joined: joined, action: action)
        button.chevron = true
        return button
    }

    private var words: String? { title?.isEmpty == true ? nil : title }

    var body: some View {
        let reach = ControlReach(size: look.size, wordless: words == nil && !chevron, margin: margin)
        Button(action: action) {
            ControlLabel(title: words, icon: icon, size: look.size, chevron: chevron, pending: pending, fills: fills)
                .padding(reach.around)
        }
        .buttonStyle(ControlButtonStyle(look: look, pending: pending, margin: reach.around))
        .padding(reach.outset)
        .allowsHitTesting(!pending)
        .help(help ?? "")
        .accessibilityLabel(words ?? help ?? "")
    }
}

/// A menu that looks and is sized as the button is.
struct ActionMenu<Content: View>: View {
    private let title: String?
    private let icon: ControlIcon?
    private let help: String?
    private let look: ControlLook
    private let chevron: Bool
    private let pending: Bool
    private let margin: EdgeInsets
    private let content: Content
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    /// A menu with words shows what is chosen, and a chevron after it.
    init(
        _ title: String?, icon: Symbol? = nil, picture: AnyView? = nil, help: String? = nil, variant: ButtonVariant = .ghost,
        size: ControlSize = .regular, pending: Bool = false, round: Bool = false, joined: HorizontalEdge.Set = [],
        margin: EdgeInsets = EdgeInsets(), @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.icon = icon.map(ControlIcon.symbol) ?? picture.map(ControlIcon.picture)
        self.help = help
        look = ControlLook(variant: variant, size: size, round: round, joined: joined)
        chevron = true
        self.pending = pending
        self.margin = margin
        self.content = content()
    }

    init(
        icon: Symbol, help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular, pending: Bool = false,
        round: Bool = false, joined: HorizontalEdge.Set = [], margin: EdgeInsets = EdgeInsets(), @ViewBuilder content: () -> Content
    ) {
        title = nil
        self.icon = .symbol(icon)
        self.help = help
        look = ControlLook(variant: variant, size: size, round: round, joined: joined)
        chevron = false
        self.pending = pending
        self.margin = margin
        self.content = content()
    }

    var body: some View {
        let reach = ControlReach(size: look.size, wordless: title == nil && !chevron, margin: margin)
        var look = self.look
        look.lit = enabled && !pending && hovering
        return Menu {
            content
        } label: {
            ControlLabel(title: title, icon: icon, size: look.size, chevron: chevron, pending: pending)
                .foregroundStyle(look.foreground)
                .padding(reach.around)
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .background { look.background.padding(reach.around) }
        .opacity(enabled || pending ? 1 : 0.45)
        .onHover { hovering = $0 }
        .padding(reach.outset)
        .allowsHitTesting(!pending)
        .help(help ?? "")
        .accessibilityLabel(title ?? help ?? "")
    }
}
