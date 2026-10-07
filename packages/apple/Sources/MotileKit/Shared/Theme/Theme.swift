import SwiftUI

/// Colours and type for the whole client. Colours are dynamic, so text built once follows the
/// appearance without being built again.
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
    static let background = dynamic(hex(0xf8f9fc), hex(0x0a0b0f))
    static let backgroundSecondary = dynamic(hex(0xeceef4), hex(0x111217))
    static let backgroundTertiary = dynamic(hex(0xe1e4ed), hex(0x191a1f))
    static let backgroundQuaternary = dynamic(hex(0xd6dae6), hex(0x212227))
    static let popover = dynamic(hex(0xffffff), hex(0x191a1f))
    static let popoverSecondary = dynamic(hex(0xeceef4), hex(0x222226))
    static let composer = dynamic(hex(0xffffff), hex(0x111217))
    static let composerSecondary = dynamic(hex(0xeceef4), hex(0x191a1f))
    static let sheet = background
    /// The composer's shadows: a small one under its box and a wide one around it and its strips.
    static let composerShadow = dynamic(hex(0x1a1f36, alpha: 0.06), hex(0x030407, alpha: 0.2))
    static let composerOutlineShadow = dynamic(hex(0x1a1f36, alpha: 0.06), hex(0x030407, alpha: 0.2))
    /// Light caught by the composer's top edge, which shows its height where a shadow can't.
    static let composerEdge = dynamic(hex(0xffffff, alpha: 0), hex(0xffffff, alpha: 0.03))
    /// Borders are solid, so that where two meet they do not darken.
    static let border = dynamic(hex(0xe2e3e5), hex(0x191a1e))
    static let borderSecondary = dynamic(hex(0xd5d6d9), hex(0x2b2b2f))
    /// A line on the system's glass, see-through so that it takes the glass's colour.
    static let glassBorder = dynamic(hex(0x000000, alpha: 0.08), hex(0xffffff, alpha: 0.08))

    #if os(macOS)
    static let systemLink = NSColor.linkColor
    #else
    static let systemLink = UIColor.link
    #endif

    // Text
    static let text = dynamic(hex(0x22242b), hex(0xdcdee4))
    static let prose = dynamic(hex(0x383b45), hex(0xa8abb6))
    static let secondary = dynamic(hex(0x6b6f7c), hex(0x9a9eab))
    static let tertiary = dynamic(hex(0x9a9eab), hex(0x646875))
    static let activity = dynamic(hex(0x6b6f7c), hex(0x7a7e8b))
    static let shimmer = dynamic(hex(0x000000), hex(0xffffff))

    // Meaning
    static let primary = dynamic(hex(0x2a5bd7), hex(0x4f7cff))
    static let primaryHover = dynamic(hex(0x2a5bd7, alpha: 0.1), hex(0x4f7cff, alpha: 0.18))
    static let link = dynamic(hex(0x1d4ed8), hex(0x7aa2ff))
    static let danger = dynamic(hex(0xc62828), hex(0xff7b72))
    static let dangerFill = dynamic(hex(0xc62828), hex(0xe5484d))
    static let dangerBackground = dynamic(hex(0xdc2626, alpha: 0.07), hex(0xff5c5c, alpha: 0.1))
    static let warning = dynamic(hex(0xb45309), hex(0xf5b454))
    static let warningBackground = dynamic(hex(0xf59e0b, alpha: 0.1), hex(0xf59e0b, alpha: 0.12))
    static let success = dynamic(hex(0x047857), hex(0x55c483))
    static let merged = dynamic(hex(0x8250df), hex(0xba93fb))
    static let working = dynamic(hex(0x0284c7), hex(0x38bdf8))
    static let monitoring = text
    static let agents = dynamic(hex(0xa16207), hex(0xe2c25b))

    // Charts: what tells one agent's line from the other's.
    static let claudeSeries = dynamic(hex(0xeb6834), hex(0xd95926))
    static let codexSeries = dynamic(hex(0x2a78d6), hex(0x3987e5))

    /// The colours code is highlighted with, by the core's palette index.
    static let syntax: [PlatformColor] = [
        text,
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

/// What a view lies on. The page is a ladder: what lies on one layer is filled with the next, and
/// so is what the pointer is over. What floats over the page, a popover or the composer, has one
/// colour of its own for both.
enum Surface {
    case background, secondary, tertiary, quaternary
    case popover, popoverSecondary
    case composer, composerSecondary
    case sheet

    /// What lies on it, and what the pointer is over.
    var next: Surface {
        switch self {
        case .background, .sheet: .secondary
        case .secondary: .tertiary
        case .tertiary: .quaternary
        case .quaternary: .tertiary
        case .popover: .popoverSecondary
        case .popoverSecondary: .popover
        case .composer: .composerSecondary
        case .composerSecondary: .composer
        }
    }

    /// What is selected, and a filled control under the pointer.
    var further: Surface {
        switch self {
        case .background, .sheet: .tertiary
        case .secondary, .popover, .popoverSecondary, .composer, .composerSecondary: .background
        case .tertiary, .quaternary: .secondary
        }
    }

    var platform: PlatformColor {
        switch self {
        case .background: Theme.background
        case .secondary: Theme.backgroundSecondary
        case .tertiary: Theme.backgroundTertiary
        case .quaternary: Theme.backgroundQuaternary
        case .popover: Theme.popover
        case .popoverSecondary: Theme.popoverSecondary
        case .composer: Theme.composer
        case .composerSecondary: Theme.composerSecondary
        case .sheet: Theme.sheet
        }
    }

    var color: Color { Color(platform: platform) }

    /// The border of what lies on it.
    var border: Color {
        switch self {
        case .background, .sheet: .themeBorder
        default: .themeBorderSecondary
        }
    }
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
            .background(surface.next.color, in: shape)
            .environment(\.surface, surface.next)
    }
}

extension View {
    /// Fills the view with what lies on the surface it is on.
    func layered(in shape: some Shape) -> some View {
        modifier(Layered(shape: shape))
    }

    /// A sheet's content: on the sheet's colour, with what lies on it filled from there.
    func sheetSurface() -> some View {
        self
            #if os(macOS)
            .background(Color.themeSheet)
            #else
            .presentationBackground(Color.themeSheet)
            #endif
            .environment(\.surface, .sheet)
    }
}

extension Color {
    static let themeBackground = Color(platform: Theme.background)
    static let themeBackgroundSecondary = Color(platform: Theme.backgroundSecondary)
    static let themeBackgroundTertiary = Color(platform: Theme.backgroundTertiary)
    static let themeBackgroundQuaternary = Color(platform: Theme.backgroundQuaternary)
    static let themePopover = Color(platform: Theme.popover)
    static let themeSheet = Color(platform: Theme.sheet)
    static let themeComposer = Color(platform: Theme.composer)
    static let themeComposerShadow = Color(platform: Theme.composerShadow)
    static let themeComposerOutlineShadow = Color(platform: Theme.composerOutlineShadow)
    static let themeComposerEdge = Color(platform: Theme.composerEdge)
    static let themeBorder = Color(platform: Theme.border)
    static let themeBorderSecondary = Color(platform: Theme.borderSecondary)
    static let themeGlassBorder = Color(platform: Theme.glassBorder)
    static let themeText = Color(platform: Theme.text)
    static let themeSecondary = Color(platform: Theme.secondary)
    static let themeTertiary = Color(platform: Theme.tertiary)
    static let themePrimary = Color(platform: Theme.primary)
    static let themePrimaryHover = Color(platform: Theme.primaryHover)
    static let themeLink = Color(platform: Theme.systemLink)
    static let themeLinkHover = Color(platform: Theme.systemLink).opacity(0.12)
    static let themeDanger = Color(platform: Theme.danger)
    static let themeDangerFill = Color(platform: Theme.dangerFill)
    static let themeWarning = Color(platform: Theme.warning)
    static let themeSuccess = Color(platform: Theme.success)
    static let themeMerged = Color(platform: Theme.merged)
    static let themeWorking = Color(platform: Theme.working)
    static let themeMonitoring = Color(platform: Theme.monitoring)
    static let themeAgents = Color(platform: Theme.agents)
    static let themeClaudeSeries = Color(platform: Theme.claudeSeries)
    static let themeCodexSeries = Color(platform: Theme.codexSeries)
}
