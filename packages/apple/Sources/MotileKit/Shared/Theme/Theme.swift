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

    private static func white(_ alpha: CGFloat) -> PlatformColor { rgb(1, 1, 1, alpha) }

    private static func rgb(_ red: CGFloat, _ green: CGFloat, _ blue: CGFloat, _ alpha: CGFloat) -> PlatformColor {
        #if os(macOS)
        NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
        #else
        UIColor(red: red, green: green, blue: blue, alpha: alpha)
        #endif
    }

    // Surfaces
    static let background = dynamic(hex(0xf8f9fc), hex(0x0a0b0f))
    static let raised = dynamic(hex(0xffffff), hex(0x15171d))
    static let sheet = dynamic(hex(0xf8f9fc), hex(0x0e1015))
    static let field = dynamic(hex(0xffffff), white(0.04))
    static let bubble = dynamic(hex(0xeceef4), hex(0x111217))
    static let composer = dynamic(hex(0xffffff), hex(0x111217))
    static let codeBlock = dynamic(hex(0xeceef4), hex(0x111217))
    static let hover = dynamic(hex(0x000000, alpha: 0.045), white(0.06))
    static let selected = dynamic(hex(0x000000, alpha: 0.08), white(0.1))
    /// Borders are solid, so that where two meet they do not darken.
    static let border = dynamic(hex(0xe2e3e5), hex(0x191a1e))
    static let strongBorder = dynamic(hex(0xd5d6d9), hex(0x2b2b2f))

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
    static let dangerBackground = dynamic(hex(0xdc2626, alpha: 0.07), hex(0xff5c5c, alpha: 0.1))
    static let warning = dynamic(hex(0xb45309), hex(0xf5b454))
    static let warningBackground = dynamic(hex(0xf59e0b, alpha: 0.1), hex(0xf59e0b, alpha: 0.12))
    static let success = dynamic(hex(0x047857), hex(0x55c483))
    static let merged = dynamic(hex(0x8250df), hex(0xba93fb))
    static let working = dynamic(hex(0x0284c7), hex(0x38bdf8))
    static let unread = dynamic(hex(0xea580c), hex(0xfb923c))

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

extension Color {
    static let themeBackground = Color(platform: Theme.background)
    static let themeRaised = Color(platform: Theme.raised)
    static let themeSheet = Color(platform: Theme.sheet)
    static let themeField = Color(platform: Theme.field)
    static let themeBubble = Color(platform: Theme.bubble)
    static let themeComposer = Color(platform: Theme.composer)
    static let themeHover = Color(platform: Theme.hover)
    static let themeSelected = Color(platform: Theme.selected)
    static let themeBorder = Color(platform: Theme.border)
    static let themeStrongBorder = Color(platform: Theme.strongBorder)
    static let themeText = Color(platform: Theme.text)
    static let themeSecondary = Color(platform: Theme.secondary)
    static let themeTertiary = Color(platform: Theme.tertiary)
    static let themePrimary = Color(platform: Theme.primary)
    static let themePrimaryHover = Color(platform: Theme.primaryHover)
    static let themeLink = Color(platform: Theme.systemLink)
    static let themeLinkHover = Color(platform: Theme.systemLink).opacity(0.12)
    static let themeDanger = Color(platform: Theme.danger)
    static let themeWarning = Color(platform: Theme.warning)
    static let themeSuccess = Color(platform: Theme.success)
    static let themeMerged = Color(platform: Theme.merged)
    static let themeWorking = Color(platform: Theme.working)
    static let themeUnread = Color(platform: Theme.unread)
}
