import SwiftUI

/// Colours and type for the whole client. Colours are dynamic, so text built once follows the
/// appearance without being built again. Every colour the clients show is one of these; a view
/// never makes one of its own, and never thins one with an opacity of its own.
enum Theme {
    static func dynamic(_ light: PlatformColor, _ dark: PlatformColor) -> PlatformColor {
        .dynamic(light, dark)
    }

    static func hex(_ value: UInt32, alpha: CGFloat = 1) -> PlatformColor {
        rgb(CGFloat((value >> 16) & 0xff) / 255, CGFloat((value >> 8) & 0xff) / 255, CGFloat(value & 0xff) / 255, alpha)
    }

    private static func rgb(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat, _ alpha: CGFloat) -> PlatformColor {
        #if os(macOS)
        NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
        #else
        UIColor(red: red, green: green, blue: blue, alpha: alpha)
        #endif
    }

    // Surfaces
    /// The window, sheets, settings and usage.
    static let background = dynamic(hex(0xf8f9fc), hex(0x0a0b0f))
    /// Anything boxed on the page: the composer, code, notices, cards, strips.
    static let card = dynamic(hex(0xeceef4), hex(0x111217))
    /// Menus and the command panel.
    static let popover = dynamic(hex(0xffffff), hex(0x191a1f))
    /// A control under the pointer, and a filled control at rest, on the page.
    static let accent = dynamic(hex(0xe1e4ed), hex(0x191a1f))
    /// A control selected or pressed, on the page.
    static let accentStronger = dynamic(hex(0xd6dae6), hex(0x212227))
    /// A row or a tab under the pointer, on the page.
    static let accentLarger = dynamic(hex(0xeceef4), hex(0x111217))
    /// A row or a tab selected, on the page.
    static let accentLargerStronger = dynamic(hex(0xe1e4ed), hex(0x191a1f))
    /// What is lit or lifted on a card.
    static let accentCard = dynamic(hex(0xe1e4ed), hex(0x191a1f))
    static let accentCardStronger = dynamic(hex(0xd6dae6), hex(0x212227))
    /// What is lit or lifted on a popover.
    static let accentPopover = dynamic(hex(0xeceef4), hex(0x222226))
    static let accentPopoverStronger = dynamic(hex(0xe1e4ed), hex(0x2b2b2f))

    // Lines
    /// Dividers on the page.
    static let border = dynamic(hex(0xe2e3e5), hex(0x191a1e))
    /// The edge of a card, a field, a popover.
    static let borderCard = dynamic(hex(0xd5d6d9), hex(0x2b2b2f))

    // Text
    static let foreground = dynamic(hex(0x22242b), hex(0xdcdee4))
    /// Secondary labels, tool rows, what the agent is doing.
    static let mutedForeground = dynamic(hex(0x6b6f7c), hex(0x9a9eab))
    /// Times, hints, counts.
    static let mutedMoreForeground = dynamic(hex(0x9a9eab), hex(0x646875))
    /// Placeholders and what is dimmed.
    static let mutedMostForeground = dynamic(hex(0xc0c3cc), hex(0x4b4e59))

    // Meaning
    static let primary = dynamic(hex(0x2a5bd7), hex(0x4f7cff))
    static let primaryForeground = hex(0xffffff)
    static let destructive = dynamic(hex(0xc62828), hex(0xff7b72))
    static let destructiveForeground = hex(0xffffff)
    static let success = dynamic(hex(0x047857), hex(0x55c483))
    static let successForeground = hex(0xffffff)
    static let warning = dynamic(hex(0xb45309), hex(0xf5b454))
    static let warningForeground = dynamic(hex(0xf8f9fc), hex(0x0a0b0f))
    static let merged = dynamic(hex(0x8250df), hex(0xba93fb))

    /// The charts' series, as light as each other, in a fixed order: Claude is the first, Codex
    /// the second.
    static let chart: [PlatformColor] = [
        dynamic(hex(0xcb5a1a), hex(0xdb703b)),
        dynamic(hex(0x3280dd), hex(0x4c94ec)),
        dynamic(hex(0x139948), hex(0x3eab5e)),
        dynamic(hex(0x9d60c6), hex(0xae75d6)),
        dynamic(hex(0x977e05), hex(0xac9008)),
    ]

    // Overlay and shadow
    /// The scrim over the page under what floats on it.
    static let overlay = dynamic(hex(0x000000, alpha: 0.32), hex(0x000000, alpha: 0.6))
    static let shadow = dynamic(hex(0x1a1f36, alpha: 0.1), hex(0x030407, alpha: 0.2))
    static let shadowStronger = dynamic(hex(0x1a1f36, alpha: 0.18), hex(0x030407, alpha: 0.4))

    // Tints
    /// How much of a colour a wash of it shows: a chip's tone, a notice, a diff's lines, a hover.
    static func tintOpacity(dark: Bool, stronger: Bool = false) -> CGFloat {
        switch (dark, stronger) {
        case (false, false): 0.1
        case (false, true): 0.22
        case (true, false): 0.14
        case (true, true): 0.3
        }
    }

    /// A wash of the colour.
    static func tint(_ color: PlatformColor, stronger: Bool = false) -> PlatformColor {
        #if os(macOS)
        NSColor(name: nil) { appearance in
            let dark = appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
            var resolved = color
            appearance.performAsCurrentDrawingAppearance { resolved = color.usingColorSpace(.sRGB) ?? color }
            return resolved.withAlphaComponent(tintOpacity(dark: dark, stronger: stronger))
        }
        #else
        UIColor { traits in
            color.resolvedColor(with: traits).withAlphaComponent(tintOpacity(dark: traits.userInterfaceStyle == .dark, stronger: stronger))
        }
        #endif
    }

    /// The colours code is highlighted with, by the core's palette index.
    static let syntax: [PlatformColor] = [
        foreground,
        dynamic(hex(0x6e7781), hex(0x8b949e)),  // comment
        dynamic(hex(0xcf222e), hex(0xff7b72)),  // keyword
        dynamic(hex(0x0a3069), hex(0xa5d6ff)),  // string
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // constant
        dynamic(hex(0x8250df), hex(0xd2a8ff)),  // function
        dynamic(hex(0x953800), hex(0xffa657)),  // type
        dynamic(hex(0x953800), hex(0xffa657)),  // variable
        dynamic(hex(0x116329), hex(0x7ee787)),  // tag
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // attribute
        dynamic(hex(0x116329), hex(0x7ee787)),  // inserted
        dynamic(hex(0xb3261e), hex(0xffa198)),  // deleted
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // heading
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // escape
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // property
        dynamic(hex(0x6e7781), hex(0x8b949e)),  // meta
    ]

    // Type
    static let proseSize: CGFloat = 14 * Platform.scale
    static let codeSize: CGFloat = 12.5 * Platform.scale
    static let proseFont = PlatformFont.systemFont(ofSize: proseSize)
    static let proseBold = PlatformFont.systemFont(ofSize: proseSize, weight: .semibold)
    static let codeFont = PlatformFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
    static let inlineCodeFont = PlatformFont.uiMono(12.5)
    static let smallFont = PlatformFont.ui(12)
    static let smallMono = PlatformFont.uiMono(11.5)
    static let proseLineHeight: CGFloat = scaled(24)
    static let codeLineHeight: CGFloat = scaled(18)

    static func heading(_ level: Int) -> PlatformFont {
        switch level {
        case 1: PlatformFont.ui(20, weight: .semibold)
        case 2: PlatformFont.ui(17, weight: .semibold)
        case 3: PlatformFont.ui(15, weight: .semibold)
        default: PlatformFont.ui(14, weight: .semibold)
        }
    }

    // Layout
    /// The widest the transcript gets.
    static let contentWidth: CGFloat = 768
    static let contentPadding: CGFloat = Platform.scale > 1 ? 16 : 38
    /// How far the composer reaches past the transcript on each side.
    static let composerReach: CGFloat = Platform.scale > 1 ? 6 : 14
    static let composerWidth = contentWidth + composerReach * 2
    static let composerPadding = contentPadding - composerReach
    /// The invisible area that takes the drag around a line that resizes.
    static let resizeGrab: CGFloat = 17
}

/// How large a control is. Its height, symbol, text, padding and corners go together, so that
/// no view picks them one by one. Sizes are as on the Mac; iOS enlarges them.
enum ControlSize {
    /// Inside a row, a tab, a chip or a card.
    case small
    /// Everywhere else.
    case regular
    /// What a screen is about, and the bars fingers press.
    case large

    var height: CGFloat {
        switch self {
        case .small: scaled(24)
        case .regular: scaled(28)
        case .large: scaled(36)
        }
    }

    var symbol: CGFloat {
        switch self {
        case .small: 13
        case .regular: 14
        case .large: 16
        }
    }

    /// A symbol that should weigh less than the others, like the x that closes a tab.
    var smallSymbol: CGFloat { symbol - 2 }

    var textSize: CGFloat {
        switch self {
        case .small: 11.5
        case .regular: 12
        case .large: 13
        }
    }

    var font: Font { .ui(size: textSize, weight: .medium) }

    /// The room between what it says and its sides.
    var padding: CGFloat {
        switch self {
        case .small: 8
        case .regular: 11
        case .large: 14
        }
    }

    /// How much nearer its side a chevron stands than words do, so that the control's sides look alike.
    var chevronOutset: CGFloat { self == .regular ? 2 : 0 }

    /// The room between its symbol and its words.
    var gap: CGFloat {
        switch self {
        case .small: 5
        case .regular: 6
        case .large: 8
        }
    }

    var radius: CGFloat {
        switch self {
        case .small: Radius.small
        case .regular: Radius.control
        case .large: Radius.large
        }
    }

    /// The side of the square its symbol is drawn in.
    var symbolSide: CGFloat { PlatformImage.symbolSide(symbol) }

    /// How much nearer its side a symbol stands than words do, so that both look as far from it.
    var symbolOutset: CGFloat { (symbolSide / 5 * 2).rounded() / 2 }
}

/// How round corners are.
enum Radius {
    static let small: CGFloat = 6
    static let control: CGFloat = 7
    static let large: CGFloat = 9
    /// Boxes that hold text or rows: cards, code, notices.
    static let card: CGFloat = 10
    static let sheet: CGFloat = 14
}

/// What a view lies on, which picks the accent of what is lit, lifted or boxed on it. A box on
/// the page is a card; a box on a card or a popover is their accent.
enum Surface {
    case background, card, popover

    var platform: PlatformColor {
        switch self {
        case .background: Theme.background
        case .card: Theme.card
        case .popover: Theme.popover
        }
    }

    var color: Color { Color(platform: platform) }

    /// A control under the pointer, and a filled control at rest.
    var accent: PlatformColor {
        switch self {
        case .background: Theme.accent
        case .card: Theme.accentCard
        case .popover: Theme.accentPopover
        }
    }

    /// A control selected or pressed.
    var accentStronger: PlatformColor {
        switch self {
        case .background: Theme.accentStronger
        case .card: Theme.accentCardStronger
        case .popover: Theme.accentPopoverStronger
        }
    }

    /// A row or a tab under the pointer.
    var rowAccent: PlatformColor { self == .background ? Theme.accentLarger : accent }

    /// A row or a tab selected.
    var rowAccentStronger: PlatformColor { self == .background ? Theme.accentLargerStronger : accentStronger }

    var accentColor: Color { Color(platform: accent) }
    var accentStrongerColor: Color { Color(platform: accentStronger) }
    var rowAccentColor: Color { Color(platform: rowAccent) }
    var rowAccentStrongerColor: Color { Color(platform: rowAccentStronger) }

    /// What lies on it: a box, a field, a code block, a chip.
    var box: PlatformColor { self == .background ? Theme.card : accent }

    var boxColor: Color { Color(platform: box) }

    /// What a box on it is, for what lies inside the box.
    var next: Surface { self == .background ? .card : self }
}

extension EnvironmentValues {
    /// The layer the view lies on.
    @Entry var surface = Surface.background
}

private struct Layered<S: Shape>: ViewModifier {
    let shape: S
    @Environment(\.surface) private var surface

    func body(content: Content) -> some View {
        content
            .background(surface.boxColor, in: shape)
            .environment(\.surface, surface.next)
    }
}

/// A wash of a colour: as much of it as the mode's tint lets through.
struct Tinted: ShapeStyle {
    let color: Color
    var stronger = false

    func resolve(in environment: EnvironmentValues) -> Color {
        color.opacity(Theme.tintOpacity(dark: environment.colorScheme == .dark, stronger: stronger))
    }
}

extension Color {
    /// A wash of the colour.
    func tinted(stronger: Bool = false) -> Tinted {
        Tinted(color: self, stronger: stronger)
    }
}

/// How far what floats stands off the page.
enum Shadow {
    /// A card, a menu, a button.
    case regular
    /// The command panel, a popover, a lifted row, the drawer.
    case stronger

    var platform: PlatformColor {
        switch self {
        case .regular: Theme.shadow
        case .stronger: Theme.shadowStronger
        }
    }

    var color: Color { Color(platform: platform) }

    var radius: CGFloat {
        switch self {
        case .regular: 12
        case .stronger: 30
        }
    }

    var down: CGFloat {
        switch self {
        case .regular: 6
        case .stronger: 14
        }
    }
}

extension View {
    /// Fills the view with what lies on the surface it is on.
    func layered(in shape: some Shape) -> some View {
        modifier(Layered(shape: shape))
    }

    /// A sheet's content: on the page's colour, with what lies on it filled from there.
    func sheetSurface() -> some View {
        self
            #if os(macOS)
            .background(Color.themeBackground)
            #else
            .presentationBackground(Color.themeBackground)
            #endif
            .environment(\.surface, .background)
    }

    /// The shadow of what floats.
    func shadow(_ shadow: Shadow) -> some View {
        self.shadow(color: shadow.color, radius: shadow.radius, y: shadow.down)
    }
}

extension Color {
    static let themeBackground = Color(platform: Theme.background)
    static let themeCard = Color(platform: Theme.card)
    static let themePopover = Color(platform: Theme.popover)
    static let themeAccent = Color(platform: Theme.accent)
    static let themeAccentStronger = Color(platform: Theme.accentStronger)
    static let themeAccentLarger = Color(platform: Theme.accentLarger)
    static let themeAccentLargerStronger = Color(platform: Theme.accentLargerStronger)
    static let themeAccentCard = Color(platform: Theme.accentCard)
    static let themeAccentCardStronger = Color(platform: Theme.accentCardStronger)
    static let themeAccentPopover = Color(platform: Theme.accentPopover)
    static let themeAccentPopoverStronger = Color(platform: Theme.accentPopoverStronger)
    static let themeBorder = Color(platform: Theme.border)
    static let themeBorderCard = Color(platform: Theme.borderCard)
    static let themeForeground = Color(platform: Theme.foreground)
    static let themeMutedForeground = Color(platform: Theme.mutedForeground)
    static let themeMutedMoreForeground = Color(platform: Theme.mutedMoreForeground)
    static let themeMutedMostForeground = Color(platform: Theme.mutedMostForeground)
    static let themePrimary = Color(platform: Theme.primary)
    static let themePrimaryForeground = Color(platform: Theme.primaryForeground)
    static let themeDestructive = Color(platform: Theme.destructive)
    static let themeDestructiveForeground = Color(platform: Theme.destructiveForeground)
    static let themeSuccess = Color(platform: Theme.success)
    static let themeSuccessForeground = Color(platform: Theme.successForeground)
    static let themeWarning = Color(platform: Theme.warning)
    static let themeWarningForeground = Color(platform: Theme.warningForeground)
    static let themeMerged = Color(platform: Theme.merged)
    static let themeChart = Theme.chart.map { Color(platform: $0) }
    static let themeOverlay = Color(platform: Theme.overlay)
}
