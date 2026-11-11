import SwiftUI

/// What a button is for, which is how it looks.
enum ButtonVariant {
    /// The one thing the view calls for.
    case primary
    /// The others beside it.
    case secondary
    /// The page's colour inside its border: the buttons of settings rows.
    case outline
    /// What can't be taken back.
    case danger
    /// What went well, or is done.
    case success
    /// What the user is asked to let happen.
    case warning
    /// What waits for something else, as the composer's buttons while their server connects.
    case waiting
    /// Nothing until the pointer is over it: the buttons of bars and rows.
    case ghost
    /// In the colour of a link.
    case link
    /// A link's colour on a wash of it: a choice that is on.
    case accent
    /// A secondary button as it is in the dark, over a picture or a video.
    case overlay
}

/// What a button shows before its words: a symbol, a picture of its own like a logo, or
/// pictures side by side, as wide as they are.
enum ControlIcon {
    case symbol(Symbol)
    case picture(AnyView)
    case pictures(AnyView)
}

/// The colours and the shape of a button or a menu. Pressed looks as under the pointer does.
struct ControlLook {
    var variant: ButtonVariant
    let size: ControlSize
    var selected = false
    var lit = false
    /// The layer it lies on.
    var surface = Surface.background
    /// The row it lies on, as it is lit.
    var row: Surface.Layer?
    /// Round ends, for the composer's buttons.
    var round = false
    /// The sides another control touches, which stay square.
    var joined: HorizontalEdge.Set = []
    /// A colour that says something, like a pull request's state, in place of the quiet ones.
    var tint: Color?
    /// On iOS's Liquid Glass, tinted in its colour, where the system has it.
    var glass = false

    var foreground: Color {
        if let tint, takesTint { return tint }
        switch variant {
        case .secondary, .outline: return .themeForeground
        case .primary: return .themePrimaryForeground
        case .danger: return .themeDestructiveForeground
        case .success: return .themeSuccessForeground
        case .warning: return .themeWarningForeground
        case .waiting: return .themePendingForeground
        case .ghost: return selected || lit ? .themeForeground : .themeMutedForeground
        case .link, .accent: return .themePrimary
        case .overlay: return .themeForegroundOverPicture
        }
    }

    /// A button filled with a colour of its own lights by that colour at opacity lit.
    private var fill: AnyShapeStyle {
        switch variant {
        case .primary: filled(.themePrimary)
        case .secondary: AnyShapeStyle(surface.color(selected || lit ? .controlLit : .control))
        case .outline: AnyShapeStyle(selected || lit ? surface.color(.control) : Color.themeBackground)
        case .danger: filled(.themeDestructive)
        case .success: filled(.themeSuccess)
        case .warning: filled(.themeWarning)
        case .waiting: filled(.themePending)
        case .ghost: AnyShapeStyle(selected ? surface.color(.controlLit) : lit ? ghostLit : .clear)
        case .link: lit ? AnyShapeStyle(Color.themePrimary.wash()) : AnyShapeStyle(Color.clear)
        case .accent: AnyShapeStyle(Color.themePrimary.wash(lit: lit))
        case .overlay: AnyShapeStyle(lit ? Color.themeControlLitOverPicture : Color.themeControlOverPicture)
        }
    }

    private func filled(_ color: Color) -> AnyShapeStyle {
        lit ? AnyShapeStyle(color.at(.lit)) : AnyShapeStyle(color)
    }

    /// A ghost button lights a step above the row it lies on.
    private var ghostLit: Color {
        switch row {
        case .rowSelected, .controlLit: surface.color(.controlLitOnSelected)
        case .row, .control: surface.color(.controlLit)
        case .chip: .themeBorderAccent
        default: surface.color(.control)
        }
    }

    var radius: CGFloat { round ? size.height / 2 : size.radius }

    fileprivate var shape: UnevenRoundedRectangle {
        let leading = joined.contains(.leading) ? 0 : radius
        let trailing = joined.contains(.trailing) ? 0 : radius
        return UnevenRoundedRectangle(
            topLeadingRadius: leading, bottomLeadingRadius: leading, bottomTrailingRadius: trailing, topTrailingRadius: trailing,
            style: .continuous)
    }

    /// The colour it is filled with, which its signal rings take.
    var fillColor: AnyShapeStyle { fill }

    /// The colour its glass is tinted in: the one it is filled with otherwise.
    var glassTint: Color {
        switch variant {
        case .primary, .link, .accent: .themePrimary
        case .danger: .themeDestructive
        case .success: .themeSuccess
        case .warning: .themeWarning
        case .waiting: .themePending
        default: .themeGlassTint
        }
    }

    var background: some View {
        shape.fill(fill)
            .overlay {
                if variant == .outline { shape.strokeBorder(surface.border, lineWidth: 1) }
                if variant == .secondary && surface.edgesControls {
                    shape.strokeBorder(surface.color(.controlLit), lineWidth: 1)
                }
                if variant == .overlay {
                    shape.strokeBorder(Color.themeControlBorderOverPicture, lineWidth: 1)
                }
            }
    }
}

extension ControlLook {
    /// How long a button's new colour takes to grow from its centre over the old one, when what it
    /// is for changes, as from waiting to sending.
    static let recolor: TimeInterval = 0.2

    /// The same look for another purpose, unlit.
    func `as`(_ variant: ButtonVariant) -> ControlLook {
        var look = self
        look.variant = variant
        look.lit = false
        return look
    }

    /// Whether it has a colour before the pointer is over it. A button that has none fades the
    /// old one out, where another grows its own over it.
    var fillsAtRest: Bool { !(variant == .ghost && !selected) && variant != .link }

    /// Whether a tint colours its words and symbol.
    var takesTint: Bool { variant == .ghost || variant == .secondary || variant == .outline }

    /// The colour of its words and symbol without a tint, for the views the transcript draws itself.
    var platformForeground: PlatformColor {
        switch variant {
        case .secondary, .outline: return Theme.foreground
        case .primary: return Theme.primaryForeground
        case .danger: return Theme.destructiveForeground
        case .success: return Theme.successForeground
        case .warning: return Theme.warningForeground
        case .waiting: return Theme.pendingForeground
        case .ghost: return selected || lit ? Theme.foreground : Theme.mutedForeground
        case .link, .accent: return Theme.primary
        case .overlay: return Theme.foregroundOverPicture
        }
    }

    var platformFill: PlatformColor {
        let filled = { (color: PlatformColor) in lit ? Theme.thinned(color, .lit) : color }
        switch variant {
        case .primary: return filled(Theme.primary)
        case .secondary: return surface.platform(selected || lit ? .controlLit : .control)
        case .outline: return selected || lit ? surface.platform(.control) : Theme.background
        case .danger: return filled(Theme.destructive)
        case .success: return filled(Theme.success)
        case .warning: return filled(Theme.warning)
        case .waiting: return filled(Theme.pending)
        case .ghost: return selected ? surface.platform(.controlLit) : lit ? platformGhostLit : .clear
        case .link: return lit ? Theme.primaryTint : .clear
        case .accent: return lit ? Theme.primaryTintStronger : Theme.primaryTint
        case .overlay: return lit ? Theme.controlLitOverPicture : Theme.controlOverPicture
        }
    }

    var platformBorder: PlatformColor? {
        switch variant {
        case .outline: surface.platformBorder
        case .secondary where surface.edgesControls: surface.platform(.controlLit)
        case .overlay: Theme.controlBorderOverPicture
        default: nil
        }
    }

    private var platformGhostLit: PlatformColor {
        switch row {
        case .rowSelected, .controlLit: surface.platform(.controlLitOnSelected)
        case .row, .control: surface.platform(.controlLit)
        case .chip: Theme.borderAccent
        default: surface.platform(.control)
        }
    }
}

/// What a button or a menu says: a symbol, words, or both. While `pending` the spinner stands
/// where the symbol is, or over the words when there is none, and the width stays. A symbol that
/// `turns` turns itself instead. With a `pendingTitle` it says that beside the spinner instead.
struct ControlLabel: View {
    let title: String?
    let icon: ControlIcon?
    let size: ControlSize
    var chevron = false
    /// What the chevron opens is open, which turns it over.
    var chevronTurned = false
    /// The chevron's colour, a step quieter than the words.
    var chevronColor = Color.themeMutedStrongerForeground
    var pending = false
    var pendingTitle: String?
    var fills = false
    /// Where its words sit when it fills.
    var alignment = HorizontalAlignment.center
    /// The symbol's or the picture's size where it isn't the one that goes with `size`.
    var symbolSize: IconSize?
    /// The room between its symbol and its words where it isn't the one that goes with `size`.
    var gap: CGFloat?
    /// The room between its words and its sides where it isn't the one that goes with `size`.
    var padding: CGFloat?
    /// A symbol after a dot behind the words, as the reasoning effort after the model.
    var detail: Symbol?
    /// Words after another dot, as the server after the effort.
    var place: String?
    /// Words after the title in a colour of their own, as "working" after "2 agents", which
    /// leave the title in the foreground.
    var tinted: (words: String, color: Color)?
    /// What the words name runs through the gateway, which its symbol after them says.
    var proxied = false
    /// What the words name is pinned to one of the gateway's accounts, which a pin after its symbol says.
    var pinned = false

    private var wordless: Bool { title == nil && !chevron }
    private var markSize: IconSize {
        guard let symbolSize else { return wordlessSymbol ? size.wordlessSymbol : size.symbol }
        return symbolSize
    }

    private var wordlessSymbol: Bool {
        guard wordless, case .symbol = icon else { return false }
        return true
    }
    private var waitingTitle: String? { pending ? pendingTitle : nil }

    var body: some View {
        HStack(spacing: size.gap) {
            if icon != nil || title != nil {
                ZStack {
                    words(title, tinted: tinted, spins: pending && icon != nil)
                        .opacity(waitingTitle == nil ? 1 : 0)
                    if let waitingTitle { words(waitingTitle, tinted: nil, spins: true) }
                }
            }
            if chevron {
                Image(.chevronDown, size: size.chevron, trimmed: true)
                    .foregroundStyle(chevronColor)
                    .rotationEffect(.degrees(chevronTurned ? 180 : 0))
                    .animation(.smooth(duration: 0.2), value: chevronTurned)
            }
        }
        .frame(maxWidth: fills ? .infinity : nil, alignment: Alignment(horizontal: alignment, vertical: .center))
        .padding(.leading, wordless ? 0 : sidePadding - (icon == nil ? 0 : size.symbolOutset) + pictureInset)
        .padding(.trailing, wordless ? 0 : sidePadding - (chevron ? size.chevronOutset : 0))
        .frame(minWidth: size.height)
        .frame(height: size.height)
        .overlay {
            if pending, icon == nil, waitingTitle == nil { Spinner(size: markSize) }
        }
    }

    private var sidePadding: CGFloat { padding ?? size.padding }

    /// A picture smaller than the square stays centred where the square would be.
    private var pictureInset: CGFloat {
        guard case .picture = icon, let symbolSize else { return 0 }
        return (size.symbolSide - PlatformImage.symbolSide(symbolSize)) / 2
    }

    private func words(_ title: String?, tinted: (words: String, color: Color)?, spins: Bool) -> some View {
        HStack(spacing: gap ?? size.gap) {
            if case .symbol(let symbol) = icon, symbol.turns {
                TurningSymbol(symbol: symbol, size: markSize, turning: pending)
            } else if spins {
                Spinner(size: markSize)
            } else if let icon {
                mark(icon)
            }
            if let title {
                HStack(spacing: 0) {
                    text(title, tinted: tinted)
                        .lineLimit(1)
                    Group {
                        if let detail {
                            Text(verbatim: " · ")
                            Image(detail, size: size.smallSymbol)
                        }
                        if let place {
                            Text(verbatim: " · ")
                            Text(verbatim: place)
                                .lineLimit(1)
                                .fixedSize()
                        }
                    }
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                }
                .font(size.font)
                .opacity(pending && !spins ? 0 : 1)
            }
        }
    }

    private func text(_ title: String, tinted: (words: String, color: Color)?) -> Text {
        let named = Text(dotted: title, proxied: proxied, pinned: pinned, fontSize: size.textSize, size: markSize)
        guard let tinted else { return named }
        return Text("\(named.foregroundStyle(Color.themeForeground)) \(Text(verbatim: tinted.words).foregroundStyle(tinted.color))")
    }

    @ViewBuilder private func mark(_ icon: ControlIcon) -> some View {
        switch icon {
        case .symbol(let symbol): Image(symbol, size: markSize)
        case .picture(let picture):
            let side = PlatformImage.symbolSide(markSize)
            picture.frame(width: side, height: side)
        case .pictures(let pictures): pictures.fixedSize()
        }
    }
}

/// How far around a control a finger still presses it, and the room that is the control's own
/// outside its background.
struct ControlReach {
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

extension EdgeInsets {
    /// Room beside a control that is its own: its half of the gap to the control before it and
    /// to the one after it, so that the gap is only seen and a press in it lands on one of them.
    static func beside(leading: CGFloat = 0, trailing: CGFloat = 0) -> EdgeInsets {
        EdgeInsets(top: 0, leading: leading, bottom: 0, trailing: trailing)
    }

    /// The room of the control at `index` of `count` side by side, whose lights stand `gap` apart.
    static func sharing(_ gap: CGFloat, at index: Int, of count: Int, along axis: Axis = .horizontal) -> EdgeInsets {
        let before = index > 0 ? gap / 2 : 0
        let after = index < count - 1 ? gap / 2 : 0
        guard axis == .vertical else { return beside(leading: before, trailing: after) }
        return EdgeInsets(top: before, leading: 0, bottom: after, trailing: 0)
    }

    /// Half of `gap` on every side, for a control among others that wrap into rows.
    static func around(_ gap: CGFloat) -> EdgeInsets {
        EdgeInsets(top: gap / 2, leading: gap / 2, bottom: gap / 2, trailing: gap / 2)
    }
}

struct ControlButtonStyle: ButtonStyle {
    let look: ControlLook
    var pending = false
    var signals = false
    var margin = EdgeInsets()

    func makeBody(configuration: Configuration) -> some View {
        ControlBody(
            label: configuration.label, pressed: configuration.isPressed, look: look, pending: pending, signals: signals, margin: margin)
    }
}

private struct ControlBody<Label: View>: View {
    let label: Label
    let pressed: Bool
    let look: ControlLook
    let pending: Bool
    let signals: Bool
    let margin: EdgeInsets
    @State private var hovering = false
    /// What it was for, which its new colour grows over: the variant it last settled on, and the one
    /// before while the new one grows.
    @State private var settled: ButtonVariant?
    @State private var grownOver: ButtonVariant?
    @State private var revealedAt: Date?
    /// The rings' look, and when they stopped starting, so that the last ones still go out.
    @State private var rings: (look: ControlLook, stopped: Date?)?
    @Environment(\.isEnabled) private var enabled
    @Environment(\.surface) private var surface
    @Environment(\.row) private var row

    private var currentLook: ControlLook {
        var look = self.look
        look.surface = surface
        look.row = row
        look.lit = enabled && !pending && (hovering || pressed)
        return look
    }

    var body: some View {
        let look = currentLook
        let dimmed = !enabled && !pending
        surfaced(look, dimmed: dimmed)
            .contentShape(Rectangle())
            .animation(.easeOut(duration: ControlLook.recolor), value: look.variant)
            .onHover { hovering = $0 }
            .onChange(of: look.variant, initial: true) { old, new in reveal(new, over: old) }
            .onChange(of: signals, initial: true) { _, signals in signal(signals, look: look) }
    }

    @ViewBuilder
    private func surfaced(_ look: ControlLook, dimmed: Bool) -> some View {
        #if os(iOS)
        if look.glass, #available(iOS 26.0, *) {
            glassed(look, dimmed: dimmed)
                .background {
                    if let rings { SignalRings(look: rings.look, stopped: rings.stopped).padding(margin) }
                }
        } else {
            filled(look).opacity(.disabled, when: dimmed)
        }
        #else
        filled(look).opacity(.disabled, when: dimmed)
        #endif
    }

    #if os(iOS)
    /// On glass its new colour is the glass's own tint, which glass blends into from the old one.
    @available(iOS 26.0, *)
    private func glassed(_ look: ControlLook, dimmed: Bool) -> some View {
        label
            .foregroundStyle(look.foreground)
            .glassEffect(.regular.tint(look.glassTint).interactive(), in: GlassShape(shape: look.shape, insets: margin))
            .opacity(.disabled, when: dimmed)
    }
    #endif

    private func filled(_ look: ControlLook) -> some View {
        label
            .foregroundStyle(look.foreground)
            .background {
                ZStack {
                    if let rings { SignalRings(look: rings.look, stopped: rings.stopped) }
                    let changing = settled != nil && settled != look.variant
                    let old = changing ? settled : grownOver
                    if old != nil || revealedAt != nil {
                        TimelineView(.animation) { context in
                            let since = revealedAt.map { context.date.timeIntervalSince($0) } ?? 0
                            let revealed = changing ? 0 : min(1, since / ControlLook.recolor)
                            ZStack {
                                if let old { look.as(old).background.opacity(look.fillsAtRest ? 1 : 1 - revealed) }
                                look.background.mask { Reveal(revealed: revealed) }
                            }
                        }
                        .transaction { $0.animation = nil }
                        .appearing(.identity)
                    } else {
                        look.background
                            .transaction { $0.animation = nil }
                            .appearing(.identity)
                    }
                }
                .compositingGroup()
                .padding(margin)
            }
    }

    private func reveal(_ variant: ButtonVariant, over old: ButtonVariant) {
        settled = variant
        guard old != variant else { return }
        grownOver = old
        revealedAt = .now
        DispatchQueue.main.asyncAfter(deadline: .now() + ControlLook.recolor) {
            guard settled == variant else { return }
            grownOver = nil
            revealedAt = nil
        }
    }

    private func signal(_ signals: Bool, look: ControlLook) {
        guard !signals else { return rings = (look, nil) }
        guard let started = rings, started.stopped == nil else { return }
        let stopped = Date.now
        rings = (started.look, stopped)
        DispatchQueue.main.asyncAfter(deadline: .now() + SignalRings.travel) {
            if rings?.stopped == stopped { rings = nil }
        }
    }
}

/// A button's shape inside its margin.
private struct GlassShape<S: Shape>: Shape {
    let shape: S
    let insets: EdgeInsets

    func path(in rect: CGRect) -> Path {
        shape.path(in: CGRect(
            x: rect.minX + insets.leading, y: rect.minY + insets.top,
            width: rect.width - insets.leading - insets.trailing, height: rect.height - insets.top - insets.bottom))
    }
}

/// A circle that grows from the middle until it covers the whole of what it masks.
private struct Reveal: View {
    let revealed: CGFloat
    /// Quick to start, then slowly settling as it reaches the edge.
    private static let curve = UnitCurve.bezier(startControlPoint: UnitPoint(x: 0.25, y: 1), endControlPoint: UnitPoint(x: 0.5, y: 1))

    var body: some View {
        GeometryReader { proxy in
            let side = hypot(proxy.size.width, proxy.size.height) * Self.curve.value(at: revealed)
            Circle()
                .frame(width: side, height: side)
                .position(x: proxy.size.width / 2, y: proxy.size.height / 2)
        }
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
    /// What it opens is open, which turns its chevron over.
    private var open = false
    private let pending: Bool
    private let pendingTitle: String?
    private let fills: Bool
    private var alignment = HorizontalAlignment.center
    private let symbolSize: IconSize?
    private var gap: CGFloat?
    private var padding: CGFloat?
    private let detail: Symbol?
    private var place: String?
    private var tinted: String?
    private var proxied = false
    private var pinned = false
    private let margin: EdgeInsets
    /// Rings move out from it, while what it does waits for something else.
    private var signals = false
    /// It stands in a line of text: its symbol or words start where the line does, and it is no
    /// taller than its words, so that nothing moves for it. Its light and its press reach around.
    private var inline = false
    private let action: () -> Void

    init(
        _ title: String, tinted: String? = nil, icon: Symbol? = nil, picture: AnyView? = nil, pictures: AnyView? = nil, detail: Symbol? = nil,
        place: String? = nil, proxied: Bool = false, pinned: Bool = false, help: String? = nil,
        variant: ButtonVariant = .secondary, size: ControlSize = .regular, symbolSize: IconSize? = nil, gap: CGFloat? = nil,
        padding: CGFloat? = nil, pending: Bool = false, pendingTitle: String? = nil,
        selected: Bool = false, round: Bool = false,
        fills: Bool = false, alignment: HorizontalAlignment = .center, opens: Bool = false, open: Bool = false, joined: HorizontalEdge.Set = [],
        signals: Bool = false, tint: Color? = nil, glass: Bool = false, margin: EdgeInsets = EdgeInsets(), inline: Bool = false,
        action: @escaping () -> Void
    ) {
        chevron = opens
        self.signals = signals
        self.open = open
        self.inline = inline
        self.title = title
        self.icon = icon.map(ControlIcon.symbol) ?? picture.map(ControlIcon.picture) ?? pictures.map(ControlIcon.pictures)
        self.detail = detail
        self.place = place
        self.tinted = tinted
        self.proxied = proxied
        self.pinned = pinned
        self.help = help
        look = ControlLook(variant: variant, size: size, selected: selected, round: round, joined: joined, tint: tint, glass: glass)
        self.pending = pending
        self.pendingTitle = pendingTitle
        self.fills = fills
        self.alignment = alignment
        self.symbolSize = symbolSize
        self.gap = gap
        self.padding = padding
        self.margin = margin
        self.action = action
    }

    /// A button that is only a symbol says what it does in `help`. `symbolSize` draws the
    /// symbol in another size in a button of the same size.
    init(
        icon: Symbol, help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular, symbolSize: IconSize? = nil,
        pending: Bool = false, selected: Bool = false, round: Bool = false, joined: HorizontalEdge.Set = [], signals: Bool = false,
        tint: Color? = nil, glass: Bool = false, margin: EdgeInsets = EdgeInsets(), inline: Bool = false, action: @escaping () -> Void
    ) {
        title = nil
        self.signals = signals
        self.inline = inline
        self.icon = .symbol(icon)
        detail = nil
        self.help = help
        look = ControlLook(variant: variant, size: size, selected: selected, round: round, joined: joined, tint: tint, glass: glass)
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
        var light = reach.around
        if inline {
            light.top += inlineVertical - Self.inlineLightRoom
            light.bottom += inlineVertical - Self.inlineLightRoom
        }
        return Button(action: action) {
            ControlLabel(
                title: words, icon: icon, size: look.size, chevron: chevron, chevronTurned: open,
                chevronColor: look.foreground == .themeForeground ? .themeMutedForeground : .themeMutedStrongerForeground, pending: pending,
                pendingTitle: pendingTitle, fills: fills,
                alignment: alignment, symbolSize: symbolSize, gap: gap, padding: padding, detail: detail, place: place,
                tinted: look.tint.flatMap { tint in tinted.map { (words: $0, color: tint) } }, proxied: proxied,
                pinned: pinned
            )
            .padding(reach.around)
        }
        .buttonStyle(ControlButtonStyle(look: look, pending: pending, signals: signals, margin: light))
        .padding(reach.outset)
        .padding(inline ? inlineBleed : EdgeInsets())
        .allowsHitTesting(!pending)
        .help(help ?? "")
        .accessibilityLabel(words.map { [$0, tinted].compactMap { $0 }.joined(separator: " ") } ?? help ?? "")
    }

    /// How far an inline button's light reaches above and below its words.
    private static let inlineLightRoom: CGFloat = 3

    /// How much taller than its words the button is, on each side.
    private var inlineVertical: CGFloat {
        let size = look.size
        return max(0, (size.height - PlatformFont.ui(size.textSize, weight: .medium).textLineHeight) / 2)
    }

    /// The room an inline button takes around its symbol and words, which the line doesn't give it.
    private var inlineBleed: EdgeInsets {
        let size = look.size
        let leading = words == nil && !chevron
            ? (size.height - PlatformImage.symbolSide(symbolSize ?? size.wordlessSymbol)) / 2
            : (padding ?? size.padding) - (icon == nil ? 0 : size.symbolOutset)
        return EdgeInsets(top: -inlineVertical, leading: -leading, bottom: -inlineVertical, trailing: 0)
    }
}

/// Rings in the button's colour that move out from behind it, a new one each `every` while the
/// last is still on its way, seen only outside it: a ring's line is what has come out past the
/// edge, up to its full width. Once they are `stopped`, the ones on their way still go out and no
/// more start.
private struct SignalRings: View {
    let look: ControlLook
    let stopped: Date?
    /// How long a ring takes to go out, and how often a new one starts.
    static let travel: TimeInterval = 1.5
    private static let every: TimeInterval = 1
    private static let spread: CGFloat = 0.22
    private static let lineWidth: CGFloat = 3
    private static let opacity = 0.6

    var body: some View {
        if !Platform.reducesMotion {
            GeometryReader { proxy in
                TimelineView(.animation) { context in
                    ZStack {
                        ForEach(Self.progresses(at: context.date, stopped: stopped), id: \.self) { progress in
                            ring(at: progress, side: min(proxy.size.width, proxy.size.height))
                        }
                    }
                }
            }
        }
    }

    /// How far each ring on its way is along it: the newest started at the last `every` before now,
    /// or before they stopped.
    private static func progresses(at date: Date, stopped: Date?) -> [Double] {
        let now = date.timeIntervalSinceReferenceDate
        let newest = ((stopped ?? date).timeIntervalSinceReferenceDate / every).rounded(.down) * every
        return (0...Int(travel / every)).compactMap { older in
            let progress = (now - newest + Double(older) * every) / travel
            return progress < 1 ? progress : nil
        }
    }

    private func ring(at progress: Double, side: CGFloat) -> some View {
        let out = Self.easeOut(progress)
        let grown = side * Self.spread * out
        return RoundedRectangle(cornerRadius: look.radius + grown, style: .continuous)
            .strokeBorder(look.fillColor, lineWidth: min(Self.lineWidth, grown))
            .padding(-grown)
            .opacity(Self.opacity * (1 - out))
    }

    private static func easeOut(_ progress: Double) -> Double {
        1 - pow(1 - progress, 3)
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
    private let symbolSize: IconSize?
    private var gap: CGFloat?
    private var padding: CGFloat?
    private let margin: EdgeInsets
    private let content: Content
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.surface) private var surface
    @Environment(\.row) private var row

    /// A menu with words shows what is chosen, and a chevron after it.
    init(
        _ title: String?, icon: Symbol? = nil, picture: AnyView? = nil, help: String? = nil, variant: ButtonVariant = .ghost,
        size: ControlSize = .regular, symbolSize: IconSize? = nil, gap: CGFloat? = nil, padding: CGFloat? = nil, pending: Bool = false,
        round: Bool = false, joined: HorizontalEdge.Set = [], tint: Color? = nil, margin: EdgeInsets = EdgeInsets(), @ViewBuilder content: () -> Content
    ) {
        self.title = title
        self.icon = icon.map(ControlIcon.symbol) ?? picture.map(ControlIcon.picture)
        self.help = help
        look = ControlLook(variant: variant, size: size, round: round, joined: joined, tint: tint)
        chevron = true
        self.pending = pending
        self.symbolSize = symbolSize
        self.gap = gap
        self.padding = padding
        self.margin = margin
        self.content = content()
    }

    /// A menu without words shows a symbol or a picture, and no chevron.
    init(
        icon: Symbol? = nil, picture: AnyView? = nil, help: String, variant: ButtonVariant = .ghost, size: ControlSize = .regular,
        symbolSize: IconSize? = nil, pending: Bool = false, round: Bool = false, joined: HorizontalEdge.Set = [], tint: Color? = nil,
        margin: EdgeInsets = EdgeInsets(), @ViewBuilder content: () -> Content
    ) {
        title = nil
        self.icon = icon.map(ControlIcon.symbol) ?? picture.map(ControlIcon.picture)
        self.help = help
        look = ControlLook(variant: variant, size: size, round: round, joined: joined, tint: tint)
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
        look.row = row
        look.lit = enabled && !pending && hovering
        return Menu {
            content
        } label: {
            ControlLabel(title: title, icon: icon, size: look.size, chevron: chevron, pending: pending, symbolSize: symbolSize, gap: gap,
                padding: padding)
                #if os(macOS)
                .foregroundStyle(look.foreground)
                #endif
                .padding(reach.around)
                .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .menuOrder(.fixed)
        #if os(iOS)
        .buttonStyle(PressedMenuLabel(look: look, enabled: enabled && !pending, margin: reach.around))
        // Its items' symbols are in the foreground wherever it lies, a settings list's tint or not.
        .tint(Color.themeForeground)
        #else
        .buttonStyle(.plain)
        #endif
        .menuIndicator(.hidden)
        .fixedSize()
        #if os(macOS)
        .background { look.background.padding(reach.around) }
        #endif
        .opacity(.disabled, when: !enabled && !pending)
        .onHover { hovering = $0 }
        .padding(reach.outset)
        .allowsHitTesting(!pending)
        .help(help ?? "")
        .accessibilityLabel(title ?? help ?? "")
    }
}

#if os(iOS)
/// A menu's label lit under a finger, as a button's is.
private struct PressedMenuLabel: ButtonStyle {
    let look: ControlLook
    let enabled: Bool
    let margin: EdgeInsets

    func makeBody(configuration: Configuration) -> some View {
        var look = self.look
        look.lit = enabled && configuration.isPressed
        return configuration.label
            .foregroundStyle(look.foreground)
            .background { look.background.padding(margin) }
    }
}
#endif

extension Text {
    /// The text with the dots between its parts muted, as "Opus 5.5 · Work".
    init(dotted text: String) {
        self.init(dotted: text.components(separatedBy: " · ").map { Text(verbatim: $0) })
    }

    /// "Opus 5.5 · Work" with the gateway's symbol after the name, ahead of the account, where it runs
    /// through the gateway, inside a line of fixed height.
    init(dotted text: String, proxied: Bool, pinned: Bool = false, fontSize: TextSize, size: IconSize? = nil) {
        let parts = text.components(separatedBy: " · ").map { Text(verbatim: $0) }
        guard proxied, let name = parts.first else {
            self.init(dotted: parts)
            return
        }
        self.init(dotted: [name.proxied(true, pinned: pinned, fontSize: fontSize, size: size)] + parts.dropFirst())
    }

    /// The gateway's symbol beside words of `fontSize` that stand alone, centred on their line, a
    /// step smaller than `size` where it goes with an icon beside it. It stands outside the line,
    /// which a symbol moved off the baseline would make taller.
    func gatewayMarked(_ proxied: Bool, pinned: Bool = false, fontSize: TextSize, size: IconSize? = nil) -> some View {
        HStack(spacing: (fontSize.rawValue / 4).rounded() + GatewayMark.gap) {
            self
            if proxied {
                let size = GatewayMark.size(for: fontSize.rawValue, beside: size)
                HStack(spacing: GatewayMark.pinGap) {
                    Image(.waypoints, size: size)
                    if pinned {
                        Image(.pin, size: size)
                    }
                }
            }
        }
    }

    /// The gateway's symbol after words of `fontSize` inside a line of fixed height, which says what
    /// they name runs through the gateway: centred on their capitals rather than standing on their
    /// baseline, and a step smaller than `size` where it goes with an icon beside it.
    func proxied(_ proxied: Bool, pinned: Bool = false, fontSize: TextSize, size: IconSize? = nil) -> Text {
        guard proxied else { return self }
        let size = GatewayMark.size(for: fontSize.rawValue, beside: size)
        let rise = ((PlatformFont.ui(fontSize).capHeight - PlatformImage.symbolSide(size)) / 2).rounded()
        let symbol = { (name: Symbol) in Text(Image(name, size: size)).baselineOffset(rise) }
        let marked = Text("\(self)\(Text(verbatim: " ").kerning(GatewayMark.gap))\(symbol(.waypoints))")
        guard pinned else { return marked }
        return Text("\(marked)\(Text(verbatim: "\u{2060}").kerning(GatewayMark.pinGap))\(symbol(.pin))")
    }

    init(dotted parts: [Text]) {
        self = parts.dropFirst().reduce(parts.first ?? Text(verbatim: "")) { line, part in
            Text("\(line)\(Text(verbatim: " · ").foregroundStyle(Color.themeMutedStrongerForeground))\(part)")
        }
    }
}

/// "Opus 5.5", the gateway's symbol where it runs through the gateway, then " · Work" and what
/// follows: the symbol after the name, ahead of the account, standing outside the line.
struct ProxiedLine: View {
    let parts: [String]
    let proxied: Bool
    /// It is pinned to one of the gateway's accounts, which a pin after the gateway's symbol says.
    var pinned = false
    let fontSize: TextSize
    var size: IconSize? = nil

    init(_ text: String, proxied: Bool, pinned: Bool = false, fontSize: TextSize, size: IconSize? = nil) {
        self.init(parts: text.components(separatedBy: " · "), proxied: proxied, pinned: pinned, fontSize: fontSize, size: size)
    }

    init(parts: [String], proxied: Bool, pinned: Bool = false, fontSize: TextSize, size: IconSize? = nil) {
        self.parts = parts
        self.proxied = proxied
        self.pinned = pinned
        self.fontSize = fontSize
        self.size = size
    }

    var body: some View {
        let words = parts.map { Text(verbatim: $0) }
        if proxied, let name = words.first {
            HStack(spacing: 0) {
                name.gatewayMarked(true, pinned: pinned, fontSize: fontSize, size: size)
                    .layoutPriority(1)
                if words.count > 1 {
                    Text(dotted: [Text(verbatim: "")] + words.dropFirst())
                }
            }
        } else {
            Text(dotted: words)
        }
    }
}
