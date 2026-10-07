import SwiftUI

/// What a button is for, which is how it looks.
enum ButtonVariant {
    /// The one thing the view calls for.
    case primary
    /// The others beside it.
    case secondary
    /// What can't be taken back.
    case danger
    /// What the user is asked to let happen.
    case warning
    /// Nothing until the pointer is over it: the buttons of bars and rows.
    case ghost
    /// In the colour of a link.
    case link
    /// A link's colour on a wash of it: a choice that is on.
    case accent
    /// White on a dark wash, over a picture or a video.
    case overlay
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
    /// The layer it lies on.
    var surface = Surface.background
    /// Round ends, for the composer's buttons.
    var round = false
    /// The sides another control touches, which stay square.
    var joined: HorizontalEdge.Set = []
    /// A colour that says something, like a pull request's state, in place of the quiet ones.
    var tint: Color?
    /// Only a symbol: too small for the next layer to show under the pointer.
    var wordless = false

    var foreground: Color {
        if let tint, variant == .ghost || variant == .secondary { return tint }
        switch variant {
        case .secondary: return .themeText
        case .primary, .danger: return .white
        case .warning: return .themeBackground
        case .ghost: return selected || lit ? .themeText : .themeSecondary
        case .link, .accent: return .themeLink
        case .overlay: return .white
        }
    }

    private var fill: Color {
        switch variant {
        case .primary: .themePrimary
        case .secondary: (selected || lit ? surface.further : surface.next).color
        case .danger: .themeDangerFill
        case .warning: .themeWarning
        case .ghost: selected ? surface.further.color : lit ? ghostLit : .clear
        case .link: lit ? .themeLinkHover : .clear
        case .accent: .themeLink.opacity(lit ? 0.22 : 0.14)
        case .overlay: .black.opacity(0.5)
        }
    }

    private var ghostLit: Color {
        guard wordless else { return surface.next.color }
        switch surface {
        case .background: return Surface.tertiary.color
        case .popover: return .themeBorderSecondary
        default: return surface.next.color
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
                if variant == .primary || variant == .danger || variant == .warning, lit { shape.fill(Color.white.opacity(0.12)) }
                if variant == .overlay { shape.fill(Color.white.opacity(lit ? 0.22 : 0.14)) }
            }
    }
}

/// What a button or a menu says: a symbol, words, or both. While `pending` the spinner stands
/// where the symbol is, or over the words when there is none, and the width stays. With a
/// `pendingTitle` it says that beside the spinner instead.
struct ControlLabel: View {
    let title: String?
    let icon: ControlIcon?
    let size: ControlSize
    var chevron = false
    var pending = false
    var pendingTitle: String?
    var fills = false
    /// Where its words sit when it fills.
    var alignment = HorizontalAlignment.center
    /// The symbol's or the picture's size where it isn't the one that goes with `size`.
    var symbolSize: CGFloat?
    /// The room between its symbol and its words where it isn't the one that goes with `size`.
    var gap: CGFloat?

    /// How much the chevron and the dots between a title's parts let through.
    private static let muted = 0.6

    private var wordless: Bool { title == nil && !chevron }
    private var markSize: CGFloat { symbolSize ?? size.symbol }
    private var waitingTitle: String? { pending ? pendingTitle : nil }

    var body: some View {
        HStack(spacing: size.gap) {
            if icon != nil || title != nil {
                ZStack {
                    words(title, spins: pending && icon != nil)
                        .opacity(waitingTitle == nil ? 1 : 0)
                    if let waitingTitle { words(waitingTitle, spins: true) }
                }
            }
            if chevron {
                Image(.chevronDown, size: size.textSize, trimmed: true)
                    .opacity(Self.muted)
            }
        }
        .frame(maxWidth: fills ? .infinity : nil, alignment: Alignment(horizontal: alignment, vertical: .center))
        .padding(.leading, wordless ? 0 : size.padding - (icon == nil ? 0 : size.symbolOutset) + pictureInset)
        .padding(.trailing, wordless ? 0 : size.padding)
        .frame(minWidth: size.height)
        .frame(height: size.height)
        .overlay {
            if pending, icon == nil, waitingTitle == nil { Spinner(size: markSize) }
        }
    }

    /// A picture smaller than the square stays centred where the square would be.
    private var pictureInset: CGFloat {
        guard case .picture = icon, let symbolSize else { return 0 }
        return (size.symbolSide - symbolSize) / 2
    }

    private func words(_ title: String?, spins: Bool) -> some View {
        HStack(spacing: gap ?? size.gap) {
            if spins {
                Spinner(size: markSize)
            } else if let icon {
                mark(icon)
            }
            if let title {
                Self.text(title)
                    .font(size.font)
                    .lineLimit(1)
                    .opacity(pending && !spins ? 0 : 1)
            }
        }
    }

    /// The title with the dots between its parts muted.
    private static func text(_ title: String) -> Text {
        let parts = title.components(separatedBy: " · ")
        return parts.dropFirst().reduce(Text(verbatim: parts[0])) { line, part in
            Text("\(line)\(Text(verbatim: " · ").foregroundStyle(.foreground.opacity(muted)))\(Text(verbatim: part))")
        }
    }

    @ViewBuilder private func mark(_ icon: ControlIcon) -> some View {
        switch icon {
        case .symbol(let symbol): Image(symbol, size: markSize)
        case .picture(let picture):
            let side = symbolSize ?? size.symbolSide
            picture.frame(width: side, height: side)
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
    @Environment(\.surface) private var surface

    var body: some View {
        var look = self.look
        look.surface = surface
        look.lit = enabled && !pending && (hovering || pressed)
        return label
            .foregroundStyle(look.foreground)
            .background { look.background.padding(margin) }
            .contentShape(Rectangle())
            .opacity(enabled || pending ? 1 : 0.45)
            .onHover { hovering = $0 }
            .background { ArrowPointer() }
    }
}

/// Keeps the arrow over a control that lies on what shows the cursor of text, also while the
/// control is disabled.
private struct ArrowPointer: View {
    var body: some View {
        #if os(macOS)
        Color.clear
            .contentShape(Rectangle())
            .textPointer(false)
            .environment(\.isEnabled, true)
        #endif
    }
}

extension ButtonStyle where Self == ControlButtonStyle {
    /// The button's look for what the system makes a button of, like a share link. Its label is
    /// a `ControlLabel` of the same size.
    static func control(_ variant: ButtonVariant = .ghost, size: ControlSize = .regular, round: Bool = false) -> ControlButtonStyle {
        ControlButtonStyle(look: ControlLook(variant: variant, size: size, round: round))
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
    private let pendingTitle: String?
    private let fills: Bool
    private var alignment = HorizontalAlignment.center
    private let symbolSize: CGFloat?
    private var gap: CGFloat?
    private let margin: EdgeInsets
    private let action: () -> Void

    init(
        _ title: String, icon: Symbol? = nil, picture: AnyView? = nil, help: String? = nil, variant: ButtonVariant = .secondary,
        size: ControlSize = .regular, symbolSize: CGFloat? = nil, gap: CGFloat? = nil, pending: Bool = false, pendingTitle: String? = nil,
        selected: Bool = false, round: Bool = false,
        fills: Bool = false, alignment: HorizontalAlignment = .center, opens: Bool = false, joined: HorizontalEdge.Set = [],
        tint: Color? = nil, margin: EdgeInsets = EdgeInsets(), action: @escaping () -> Void
    ) {
        chevron = opens
        self.title = title
        self.icon = icon.map(ControlIcon.symbol) ?? picture.map(ControlIcon.picture)
        self.help = help
        look = ControlLook(variant: variant, size: size, selected: selected, round: round, joined: joined, tint: tint)
        self.pending = pending
        self.pendingTitle = pendingTitle
        self.fills = fills
        self.alignment = alignment
        self.symbolSize = symbolSize
        self.gap = gap
        self.margin = margin
        self.action = action
    }

    /// A button that is only a symbol says what it does in `help`. `symbolSize` draws the
    /// symbol in another size in a button of the same size.
    init(
        icon: Symbol, help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular, symbolSize: CGFloat? = nil,
        pending: Bool = false, selected: Bool = false, round: Bool = false, joined: HorizontalEdge.Set = [], tint: Color? = nil,
        margin: EdgeInsets = EdgeInsets(), action: @escaping () -> Void
    ) {
        title = nil
        self.icon = .symbol(icon)
        self.help = help
        look = ControlLook(variant: variant, size: size, selected: selected, round: round, joined: joined, tint: tint, wordless: true)
        self.pending = pending
        pendingTitle = nil
        fills = false
        self.symbolSize = symbolSize
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
            ControlLabel(
                title: words, icon: icon, size: look.size, chevron: chevron, pending: pending, pendingTitle: pendingTitle, fills: fills,
                alignment: alignment, symbolSize: symbolSize, gap: gap
            )
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
    private let symbolSize: CGFloat?
    private var gap: CGFloat?
    private let margin: EdgeInsets
    private let content: Content
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.surface) private var surface

    /// A menu with words shows what is chosen, and a chevron after it.
    init(
        _ title: String?, icon: Symbol? = nil, picture: AnyView? = nil, help: String? = nil, variant: ButtonVariant = .ghost,
        size: ControlSize = .regular, symbolSize: CGFloat? = nil, gap: CGFloat? = nil, pending: Bool = false, round: Bool = false,
        joined: HorizontalEdge.Set = [], tint: Color? = nil, margin: EdgeInsets = EdgeInsets(), @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.icon = icon.map(ControlIcon.symbol) ?? picture.map(ControlIcon.picture)
        self.help = help
        look = ControlLook(variant: variant, size: size, round: round, joined: joined, tint: tint)
        chevron = true
        self.pending = pending
        self.symbolSize = symbolSize
        self.gap = gap
        self.margin = margin
        self.content = content()
    }

    /// A menu without words shows a symbol or a picture, and no chevron.
    init(
        icon: Symbol? = nil, picture: AnyView? = nil, help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular,
        symbolSize: CGFloat? = nil, pending: Bool = false, round: Bool = false, joined: HorizontalEdge.Set = [], tint: Color? = nil,
        margin: EdgeInsets = EdgeInsets(), @ViewBuilder content: () -> Content
    ) {
        title = nil
        self.icon = icon.map(ControlIcon.symbol) ?? picture.map(ControlIcon.picture)
        self.help = help
        look = ControlLook(variant: variant, size: size, round: round, joined: joined, tint: tint, wordless: true)
        chevron = false
        self.pending = pending
        self.symbolSize = symbolSize
        self.margin = margin
        self.content = content()
    }

    var body: some View {
        let reach = ControlReach(size: look.size, wordless: title == nil && !chevron, margin: margin)
        var look = self.look
        look.surface = surface
        look.lit = enabled && !pending && hovering
        return Menu {
            content
        } label: {
            ControlLabel(title: title, icon: icon, size: look.size, chevron: chevron, pending: pending, symbolSize: symbolSize, gap: gap)
                .foregroundStyle(look.foreground)
                .padding(reach.around)
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .menuOrder(.fixed)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .background { look.background.padding(reach.around) }
        .opacity(enabled || pending ? 1 : 0.45)
        .onHover { hovering = $0 }
        .background { ArrowPointer() }
        .padding(reach.outset)
        .allowsHitTesting(!pending)
        .help(help ?? "")
        .accessibilityLabel(title ?? help ?? "")
    }
}
