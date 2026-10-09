import SwiftUI

/// The design tokens, generated into `Tokens.swift` from `packages/theme/tokens.json`: every
/// colour the clients show, one set for both appearances, with `Opacity`, `ShadowSize` and
/// `Radius` beside it. Colours are dynamic, so text built once follows the appearance without
/// being built again. A view never makes a colour of its own.
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

    static func tone(_ light: UInt32, _ dark: UInt32) -> PlatformColor {
        dynamic(hex(light), hex(dark))
    }

    /// The colour as it is in one appearance.
    static func resolved(_ color: PlatformColor, dark: Bool) -> PlatformColor {
        #if os(macOS)
        var resolved = color
        NSAppearance(named: dark ? .darkAqua : .aqua)?.performAsCurrentDrawingAppearance {
            resolved = NSColor(cgColor: color.cgColor) ?? color
        }
        return resolved
        #else
        color.resolvedColor(with: UITraitCollection(userInterfaceStyle: dark ? .dark : .light))
        #endif
    }

    /// The colour thinned to one of the opacities, in each appearance to that appearance's value.
    static func thinned(_ color: PlatformColor, _ opacity: Opacity) -> PlatformColor {
        dynamic(
            resolved(color, dark: false).withAlphaComponent(opacity.light),
            resolved(color, dark: true).withAlphaComponent(opacity.dark))
    }

    /// The text colours as they are in the dark, for what lies over a picture.
    static let foregroundOverPicture = resolved(foreground, dark: true)
    static let mutedForegroundOverPicture = resolved(mutedForeground, dark: true)
    /// A colour's wash: the colour at color-tint. Chips, notices, diff lines.
    static let primaryTint = thinned(primary, .colorTint)
    static let primaryTintStronger = thinned(primary, .colorTintStronger)
    static let destructiveTint = thinned(destructive, .colorTint)
    static let successTint = thinned(success, .colorTint)
    static let warningTint = thinned(warning, .colorTint)
    /// The scrim at its opacity.
    static let scrim = thinned(overlay, .overlay)

    /// The colours code is highlighted with, by the core's palette index.
    static let syntax: [PlatformColor] = [
        foreground,
        syntaxComment,
        syntaxKeyword,
        syntaxString,
        syntaxConstant,
        syntaxFunction,
        syntaxType,
        syntaxType,  // variable
        syntaxTag,
        syntaxConstant,  // attribute
        success,  // inserted
        destructive,  // deleted
        syntaxConstant,  // heading
        syntaxConstant,  // escape
        syntaxConstant,  // property
        syntaxComment,  // meta
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

extension Opacity {
    func value(for scheme: ColorScheme) -> Double {
        scheme == .dark ? dark : light
    }

    /// The opacity where it is the same in both appearances.
    var fixed: CGFloat {
        assert(light == dark)
        return light
    }
}

extension ShadowSize {
    /// A SwiftUI or Core Animation radius, half the blur.
    var radius: CGFloat { blur / 2 }
}

/// A colour at one of the opacities, which can differ between the appearances.
struct ThinnedColor: ShapeStyle {
    let color: Color
    let opacity: Opacity

    func resolve(in environment: EnvironmentValues) -> Color {
        color.opacity(opacity.value(for: environment.colorScheme))
    }
}

extension Color {
    /// The colour at one of the opacities.
    func at(_ opacity: Opacity) -> ThinnedColor {
        ThinnedColor(color: self, opacity: opacity)
    }

    /// The colour's wash: at color-tint, or color-tint-stronger when lit.
    func wash(lit: Bool = false) -> ThinnedColor {
        at(lit ? .colorTintStronger : .colorTint)
    }
}

private struct Thinned: ViewModifier {
    let opacity: Opacity
    let on: Bool
    @Environment(\.colorScheme) private var scheme

    func body(content: Content) -> some View {
        content.opacity(on ? opacity.value(for: scheme) : 1)
    }
}

extension View {
    /// Thins the whole view to one of the opacities, while `on`.
    func opacity(_ opacity: Opacity, when on: Bool = true) -> some View {
        modifier(Thinned(opacity: opacity, on: on))
    }

    /// A shadow of the size, in the colour shadow at one of its three opacities.
    func shadow(_ size: ShadowSize, _ strength: Opacity = .shadow) -> some View {
        shadow(color: Color(platform: Theme.thinned(Theme.shadow, strength)), radius: size.radius, y: size.down)
    }
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
        case .small: Radius.sm
        case .regular, .large: Radius.md
        }
    }

    /// The side of the square its symbol is drawn in.
    var symbolSide: CGFloat { PlatformImage.symbolSide(symbol) }

    /// How much nearer its side a symbol stands than words do, so that both look as far from it.
    var symbolOutset: CGFloat { (symbolSide / 5 * 2).rounded() / 2 }
}

/// What a view lies on: the page, a box on it, a card or a popover. What is drawn on a surface
/// takes the surface's accent.
enum Surface {
    case background, secondary, card, popover

    /// What is drawn on a surface, each in the surface's own accent.
    enum Layer {
        /// A filled control at rest: a secondary button, a chip's fill, a track.
        case control
        /// The same lit, or selected.
        case controlLit
        /// A control lit on a selected row, which is already as light as a lit control.
        case controlLitOnSelected
        /// A row or a tab lit.
        case row
        /// A row or a tab selected.
        case rowSelected
        /// A box on it: code, a message, a notice, a quoted line.
        case box
    }

    var platform: PlatformColor {
        switch self {
        case .background: Theme.background
        case .secondary: Theme.backgroundSecondary
        case .card: Theme.card
        case .popover: Theme.popover
        }
    }

    var color: Color { Color(platform: platform) }

    /// The surface's edge, which the lines across it share.
    var border: Color {
        switch self {
        case .background, .secondary: .themeBorder
        case .card: .themeBorderCard
        case .popover: .themeBorderPopover
        }
    }

    func platform(_ layer: Layer) -> PlatformColor {
        switch self {
        case .background:
            switch layer {
            case .control: Theme.backgroundAccent
            case .controlLit: Theme.backgroundAccentStronger
            case .controlLitOnSelected: Theme.backgroundAccentStrongest
            case .row: Theme.backgroundAccentLarger
            case .rowSelected: Theme.backgroundAccentLargerStronger
            case .box: Theme.backgroundSecondary
            }
        case .secondary:
            switch layer {
            case .control, .row, .box: Theme.backgroundSecondaryAccent
            case .controlLit, .controlLitOnSelected, .rowSelected: Theme.backgroundSecondaryAccentStronger
            }
        case .card: Theme.cardAccent
        case .popover: Theme.popoverAccent
        }
    }

    func color(_ layer: Layer) -> Color { Color(platform: platform(layer)) }

    /// What lies in a box on it.
    var inBox: Surface {
        self == .background ? .secondary : self
    }
}

extension EnvironmentValues {
    /// The layer the view lies on.
    @Entry var surface = Surface.background
    /// The row the view lies on, as it is lit, so that a control on it lights a step above it.
    @Entry var row: Surface.Layer?
}

private struct Box<S: Shape>: ViewModifier {
    let shape: S
    @Environment(\.surface) private var surface

    func body(content: Content) -> some View {
        content
            .background(surface.color(.box), in: shape)
            .environment(\.surface, surface.inBox)
    }
}

extension View {
    /// Makes the view a box on the surface it is on.
    func box(in shape: some Shape) -> some View {
        modifier(Box(shape: shape))
    }

    /// A sheet's content: on the background, with what lies on it filled from there.
    func sheetSurface() -> some View {
        self
            #if os(macOS)
            .background(Color.themeBackground)
            #else
            .presentationBackground(Color.themeBackground)
            #endif
            .environment(\.surface, .background)
    }
}

extension Color {
    static let themeForegroundOverPicture = Color(platform: Theme.foregroundOverPicture)
    static let themeMutedForegroundOverPicture = Color(platform: Theme.mutedForegroundOverPicture)
    static let themeScrim = Color(platform: Theme.scrim)
}
