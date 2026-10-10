// The design tokens of the clients. Generated from tokens.json by build.mjs: edit tokens.json and run `pnpm tokens`.
import SwiftUI

extension Theme {
    static let background = tone(0xf8f9fc, 0x0a0b0f)
    static let backgroundAccent = tone(0xe6e8ef, 0x191a1f)
    static let backgroundAccentStronger = tone(0xdbdee8, 0x212329)
    static let backgroundAccentStrongest = tone(0xd0d4e0, 0x2b2c32)
    static let backgroundAccentLarger = tone(0xeaecf2, 0x14151a)
    static let backgroundAccentLargerStronger = tone(0xe1e4ed, 0x191a1f)
    static let backgroundSecondary = tone(0xeceef4, 0x111217)
    static let backgroundSecondaryAccent = tone(0xe1e4ed, 0x191a1f)
    static let backgroundSecondaryAccentStronger = tone(0xd6dae6, 0x212227)
    static let card = tone(0xf3f4f8, 0x15161b)
    static let cardAccent = tone(0xe8eaf0, 0x1e1f26)
    static let popover = tone(0xffffff, 0x191a1f)
    static let popoverAccent = tone(0xeceef4, 0x212329)
    static let foreground = tone(0x22242b, 0xd0d2d9)
    static let emphasizedForeground = tone(0x000000, 0xffffff)
    static let mutedForeground = tone(0x6b6f7c, 0x9a9eab)
    static let mutedStrongerForeground = tone(0x9a9eab, 0x646875)
    static let mutedStrongestForeground = tone(0xb8bcc8, 0x4a4e5a)
    static let primary = tone(0x2a5bd7, 0x4f7cff)
    static let primaryForeground = tone(0xffffff, 0xffffff)
    static let destructive = tone(0xc62828, 0xdf6c6a)
    static let destructiveForeground = tone(0xffffff, 0xffffff)
    static let success = tone(0x047857, 0x41b76f)
    static let successForeground = tone(0xffffff, 0xffffff)
    static let warning = tone(0xc74f0a, 0xe98538)
    static let warningForeground = tone(0xffffff, 0xffffff)
    static let pending = tone(0x947100, 0xd3b142)
    static let pendingForeground = tone(0xffffff, 0xffffff)
    static let process = tone(0x0284c7, 0x2aa9e5)
    static let processForeground = tone(0xffffff, 0xffffff)
    static let merged = tone(0x8250df, 0xb480ff)
    static let mergedForeground = tone(0xffffff, 0xffffff)
    static let border = tone(0xe2e3e5, 0x191a1e)
    static let borderCard = tone(0xe6e8ee, 0x1e1f25)
    static let borderInput = tone(0xeceef4, 0x1a1b21)
    static let borderPopover = tone(0xe8eaef, 0x222228)
    static let input = tone(0xeaecf2, 0x14151a)
    static let composer = tone(0xffffff, 0x111217)
    static let ring = tone(0x22242b, 0xd0d2d9)
    static let overlay = tone(0x000000, 0x000000)
    static let shadow = tone(0x000000, 0x000000)
    static let chart1 = tone(0x2a5bd7, 0x4f7cff)
    static let chart2 = tone(0x047857, 0x41b76f)
    static let chart3 = tone(0xc74f0a, 0xe98538)
    static let chart4 = tone(0x8250df, 0xb480ff)
    static let chart5 = tone(0x0284c7, 0x2aa9e5)
    static let claude = tone(0xd97757, 0xd97757)
    static let openai = tone(0x22242b, 0xd0d2d9)
    static let googleBlue = tone(0x4285f4, 0x4285f4)
    static let googleRed = tone(0xea4335, 0xea4335)
    static let googleYellow = tone(0xfbbc05, 0xfbbc05)
    static let googleGreen = tone(0x34a853, 0x34a853)
    static let linear = tone(0x22242b, 0xd0d2d9)
    static let windowClose = tone(0xff5f57, 0xff5f57)
    static let windowMinimize = tone(0xfebc2e, 0xfebc2e)
    static let windowZoom = tone(0x28c840, 0x28c840)
    static let syntaxComment = tone(0x6e7781, 0x8b949e)
    static let syntaxKeyword = tone(0xcf222e, 0xff7b72)
    static let syntaxString = tone(0x0a3069, 0xa5d6ff)
    static let syntaxConstant = tone(0x0550ae, 0x79c0ff)
    static let syntaxFunction = tone(0x8250df, 0xd2a8ff)
    static let syntaxType = tone(0x953800, 0xffa657)
    static let syntaxTag = tone(0x116329, 0x7ee787)
}

extension Color {
    static let themeBackground = Color(platform: Theme.background)
    static let themeBackgroundAccent = Color(platform: Theme.backgroundAccent)
    static let themeBackgroundAccentStronger = Color(platform: Theme.backgroundAccentStronger)
    static let themeBackgroundAccentStrongest = Color(platform: Theme.backgroundAccentStrongest)
    static let themeBackgroundAccentLarger = Color(platform: Theme.backgroundAccentLarger)
    static let themeBackgroundAccentLargerStronger = Color(platform: Theme.backgroundAccentLargerStronger)
    static let themeBackgroundSecondary = Color(platform: Theme.backgroundSecondary)
    static let themeBackgroundSecondaryAccent = Color(platform: Theme.backgroundSecondaryAccent)
    static let themeBackgroundSecondaryAccentStronger = Color(platform: Theme.backgroundSecondaryAccentStronger)
    static let themeCard = Color(platform: Theme.card)
    static let themeCardAccent = Color(platform: Theme.cardAccent)
    static let themePopover = Color(platform: Theme.popover)
    static let themePopoverAccent = Color(platform: Theme.popoverAccent)
    static let themeForeground = Color(platform: Theme.foreground)
    static let themeEmphasizedForeground = Color(platform: Theme.emphasizedForeground)
    static let themeMutedForeground = Color(platform: Theme.mutedForeground)
    static let themeMutedStrongerForeground = Color(platform: Theme.mutedStrongerForeground)
    static let themeMutedStrongestForeground = Color(platform: Theme.mutedStrongestForeground)
    static let themePrimary = Color(platform: Theme.primary)
    static let themePrimaryForeground = Color(platform: Theme.primaryForeground)
    static let themeDestructive = Color(platform: Theme.destructive)
    static let themeDestructiveForeground = Color(platform: Theme.destructiveForeground)
    static let themeSuccess = Color(platform: Theme.success)
    static let themeSuccessForeground = Color(platform: Theme.successForeground)
    static let themeWarning = Color(platform: Theme.warning)
    static let themeWarningForeground = Color(platform: Theme.warningForeground)
    static let themePending = Color(platform: Theme.pending)
    static let themePendingForeground = Color(platform: Theme.pendingForeground)
    static let themeProcess = Color(platform: Theme.process)
    static let themeProcessForeground = Color(platform: Theme.processForeground)
    static let themeMerged = Color(platform: Theme.merged)
    static let themeMergedForeground = Color(platform: Theme.mergedForeground)
    static let themeBorder = Color(platform: Theme.border)
    static let themeBorderCard = Color(platform: Theme.borderCard)
    static let themeBorderInput = Color(platform: Theme.borderInput)
    static let themeBorderPopover = Color(platform: Theme.borderPopover)
    static let themeInput = Color(platform: Theme.input)
    static let themeComposer = Color(platform: Theme.composer)
    static let themeRing = Color(platform: Theme.ring)
    static let themeOverlay = Color(platform: Theme.overlay)
    static let themeShadow = Color(platform: Theme.shadow)
    static let themeChart1 = Color(platform: Theme.chart1)
    static let themeChart2 = Color(platform: Theme.chart2)
    static let themeChart3 = Color(platform: Theme.chart3)
    static let themeChart4 = Color(platform: Theme.chart4)
    static let themeChart5 = Color(platform: Theme.chart5)
    static let themeClaude = Color(platform: Theme.claude)
    static let themeOpenai = Color(platform: Theme.openai)
    static let themeGoogleBlue = Color(platform: Theme.googleBlue)
    static let themeGoogleRed = Color(platform: Theme.googleRed)
    static let themeGoogleYellow = Color(platform: Theme.googleYellow)
    static let themeGoogleGreen = Color(platform: Theme.googleGreen)
    static let themeLinear = Color(platform: Theme.linear)
    static let themeWindowClose = Color(platform: Theme.windowClose)
    static let themeWindowMinimize = Color(platform: Theme.windowMinimize)
    static let themeWindowZoom = Color(platform: Theme.windowZoom)
    static let themeSyntaxComment = Color(platform: Theme.syntaxComment)
    static let themeSyntaxKeyword = Color(platform: Theme.syntaxKeyword)
    static let themeSyntaxString = Color(platform: Theme.syntaxString)
    static let themeSyntaxConstant = Color(platform: Theme.syntaxConstant)
    static let themeSyntaxFunction = Color(platform: Theme.syntaxFunction)
    static let themeSyntaxType = Color(platform: Theme.syntaxType)
    static let themeSyntaxTag = Color(platform: Theme.syntaxTag)
}

/// How much of a colour shows: every opacity in the clients is one of these.
enum Opacity {
    case overlay, overlayPulled, disabled, lit, shadow, shadowStronger, shadowStrongest, colorTint, colorTintStronger, colorTintChart, inputGlassTint, shimmerBandEdge, shimmerBandMiddle, glow, glowFaint, glowBright

    var light: CGFloat {
        switch self {
        case .overlay: 0.5
        case .overlayPulled: 0.4
        case .disabled: 0.45
        case .lit: 0.88
        case .shadow: 0.06
        case .shadowStronger: 0.14
        case .shadowStrongest: 0.25
        case .colorTint: 0.12
        case .colorTintStronger: 0.2
        case .colorTintChart: 0.25
        case .inputGlassTint: 0.8
        case .shimmerBandEdge: 0.12
        case .shimmerBandMiddle: 0.55
        case .glow: 0.4
        case .glowFaint: 0.3
        case .glowBright: 0.5
        }
    }

    var dark: CGFloat {
        switch self {
        case .overlay: 0.6
        case .overlayPulled: 0.4
        case .disabled: 0.45
        case .lit: 0.88
        case .shadow: 0.2
        case .shadowStronger: 0.4
        case .shadowStrongest: 0.6
        case .colorTint: 0.1
        case .colorTintStronger: 0.16
        case .colorTintChart: 0.2
        case .inputGlassTint: 0.8
        case .shimmerBandEdge: 0.12
        case .shimmerBandMiddle: 0.55
        case .glow: 0.4
        case .glowFaint: 0.3
        case .glowBright: 0.5
        }
    }
}

/// How round corners are: every radius is one of these.
enum Radius {
    static let xs: CGFloat = 4
    static let sm: CGFloat = 6
    static let md: CGFloat = 8
    static let lg: CGFloat = 12
    static let xl: CGFloat = 16
    static let xxl: CGFloat = 24
}

/// A shadow's size: how far down it falls and its blur, as CSS gives them.
enum ShadowSize {
    case sm, md, lg, xl

    var down: CGFloat {
        switch self {
        case .sm: 2.5
        case .md: 4
        case .lg: 8
        case .xl: 16
        }
    }

    var blur: CGFloat {
        switch self {
        case .sm: 8
        case .md: 12
        case .lg: 24
        case .xl: 40
        }
    }
}
