import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The constants of `crates/openui/LAYOUT.md` that the tree doesn't carry: the type, the paddings
/// of the narrow column and what the components are made of.
enum ProgramStyle {
    /// Under this column width the narrow rules apply.
    static let narrowBelow: CGFloat = 640

    static func cardPadding(narrow: Bool) -> CGFloat { narrow ? 16 : 20 }
    static let nestedCardPadding: CGFloat = 16
    static let labelGap: CGFloat = 8
    static let errorGap: CGFloat = 4
    static let popoverGap: CGFloat = 4
    static let rowSpace: CGFloat = 12

    /// A text style as the client sets it: its font, the height of its lines and its colour
    /// when the tree gives none.
    struct Look {
        let font: PlatformFont
        let lineHeight: CGFloat
    }

    static func look(_ style: UiTextStyle) -> Look {
        switch style {
        case .body: Look(font: Theme.proseFont, lineHeight: Theme.proseLineHeight)
        case .bodyCard: Look(font: Theme.proseFont, lineHeight: scaled(22))
        case .large: Look(font: .ui(.lg), lineHeight: scaled(26))
        case .small: Look(font: .ui(.sm), lineHeight: scaled(20))
        case .caption: Look(font: Theme.smallFont, lineHeight: scaled(18))
        case .label: Look(font: .ui(.sm, weight: .medium), lineHeight: scaled(18))
        case .titleSm: Look(font: .uiDigits(Theme.headingSize(4), weight: .semibold), lineHeight: scaled(20))
        case .titleMd: Look(font: .uiDigits(Theme.headingSize(3), weight: .semibold), lineHeight: scaled(22))
        case .titleLg, .word: Look(font: .uiDigits(Theme.headingSize(2), weight: .semibold), lineHeight: scaled(24))
        case .titleXl: Look(font: .uiDigits(Theme.headingSize(1), weight: .semibold), lineHeight: scaled(26))
        case .title2xl, .title3xl: Look(font: .uiDigits(.xxl, weight: .semibold), lineHeight: scaled(32))
        case .cardTitle: Look(font: .uiDigits(.md, weight: .medium), lineHeight: scaled(20))
        case .cardSubtitle: Look(font: .ui(.sm), lineHeight: scaled(18))
        case .cardHeading: Look(font: .uiDigits(.lg, weight: .semibold), lineHeight: scaled(22))
        case .readout: Look(font: .uiDigits(.xs, weight: .medium), lineHeight: scaled(18))
        case .tick: Look(font: .uiDigits(.xs), lineHeight: scaled(16))
        case .timer: Look(font: .uiDigits(.xxl, weight: .semibold), lineHeight: scaled(32))
        case .code: Look(font: Theme.codeFont, lineHeight: Theme.codeLineHeight)
        case .wordBack: Look(font: .ui(.md, weight: .medium), lineHeight: Theme.proseLineHeight)
        }
    }

    /// The colour, on the surface the node lies on. `current` is the text's own colour there.
    static func color(_ color: UiColor, on surface: Surface, current: PlatformColor = Theme.foreground) -> PlatformColor {
        switch color {
        case .current: current
        case .token(let name): Theme.named(name) ?? current
        case .wash(let name): Theme.thinned(Theme.named(name) ?? current, .colorTint)
        case .surface(let layer): Self.layer(layer, of: surface)
        case .rgba(let channels):
            rgba(channels)
        }
    }

    static func layer(_ layer: String, of surface: Surface) -> PlatformColor {
        switch layer {
        case "border": surface.platformBorder
        case "row": surface.platform(.row)
        case "control": surface.platform(.control)
        case "box": surface.platform(.box)
        default: surface.platform
        }
    }

    private static func rgba(_ channels: [Int]) -> PlatformColor {
        let value = channels + [255, 255, 255, 255].dropFirst(channels.count)
        let (red, green, blue, alpha) = (CGFloat(value[0]) / 255, CGFloat(value[1]) / 255, CGFloat(value[2]) / 255, CGFloat(value[3]) / 255)
        #if os(macOS)
        return NSColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
        #else
        return UIColor(red: red, green: green, blue: blue, alpha: alpha)
        #endif
    }

    static func radius(_ radius: UiRadius, side: CGFloat) -> CGFloat {
        switch radius.kind {
        case .xs: Radius.xs
        case .sm: Radius.sm
        case .md: Radius.md
        case .lg: Radius.lg
        case .xl: Radius.xl
        case .full: side / 2
        case .points: radius.points
        }
    }

    static func controlSize(_ size: UiControlSize) -> ControlSize {
        switch size {
        case .small: .small
        case .regular: .regular
        case .large: .large
        }
    }

    /// The kit's button for a program's: a coloured one takes the colour as its tint.
    static func variant(_ variant: UiButtonVariant) -> ButtonVariant {
        switch variant {
        case .primary: .primary
        case .danger: .danger
        case .success: .success
        case .outline: .outline
        case .secondary: .secondary
        case .ghost: .ghost
        }
    }
}
