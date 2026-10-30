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
    /// A control as it is in the dark, over a picture: the background's accent, stronger when lit.
    static let controlOverPicture = resolved(backgroundAccent, dark: true)
    static let controlLitOverPicture = resolved(backgroundAccentStronger, dark: true)
    /// A colour's wash: the colour at color-tint. Chips, notices, diff lines.
    static let primaryTint = thinned(primary, .colorTint)
    static let primaryTintStronger = thinned(primary, .colorTintStronger)
    static let destructiveTint = thinned(destructive, .colorTint)
    static let successTint = thinned(success, .colorTint)
    static let warningTint = thinned(warning, .colorTint)
    /// The scrim at its opacity.
    static let scrim = thinned(overlay, .overlay)
    /// The glass's tint: its colour at its opacity.
    static let glassTint = thinned(glass, .glassTint)
    /// What is drawn on glass, which shows through it: the accent at its opacity.
    static let accentWithOpacityTint = thinned(accentWithOpacity, .accentOpacity)

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

extension IconSize {
    /// The icon that goes with text of `size`: about a fifth larger, as on the web.
    init(text size: CGFloat) {
        switch size {
        case ..<11: self = .xs
        case ..<13: self = .sm
        case ..<15: self = .md
        case ..<17: self = .lg
        case ..<20: self = .xl
        default: self = .xxl
        }
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

    /// Its symbol, as large as an icon beside its text is.
    var symbol: IconSize { IconSize(text: textSize) }

    /// A symbol that should weigh less than the others, like the x that closes a tab.
    var smallSymbol: IconSize {
        switch self {
        case .small, .regular: .xs
        case .large: .sm
        }
    }

    /// The chevron that opens a menu, trimmed to what it draws.
    var chevron: IconSize {
        switch self {
        case .small, .regular: .sm
        case .large: .md
        }
    }

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

/// What a view lies on: the page, a box on it, a card, a popover, the composer or glass. What is drawn
/// on a surface takes the surface's accent.
enum Surface {
    case background, secondary, card, popover, composer, glass

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
        /// A chip's fill, edged in the accent border, which a control on it lights by.
        case chip
    }

    var platform: PlatformColor {
        switch self {
        case .background: Theme.background
        case .secondary: Theme.backgroundSecondary
        case .card: Theme.card
        case .popover: Theme.popover
        case .composer: Theme.composer
        case .glass: Theme.glassTint
        }
    }

    var color: Color { Color(platform: platform) }

    /// The surface's edge, which the lines across it share.
    var border: Color {
        switch self {
        case .background, .secondary: .themeBorder
        case .card: .themeBorderCard
        case .popover: .themeBorderPopover
        case .composer, .glass: .themeBorderComposer
        }
    }

    var platformBorder: PlatformColor {
        switch self {
        case .background, .secondary: Theme.border
        case .card: Theme.borderCard
        case .popover: Theme.borderPopover
        case .composer, .glass: Theme.borderComposer
        }
    }

    func platform(_ layer: Layer) -> PlatformColor {
        switch self {
        case .background:
            switch layer {
            case .control, .chip: Theme.backgroundAccent
            case .controlLit: Theme.backgroundAccentStronger
            case .controlLitOnSelected: Theme.backgroundAccentStrongest
            case .row: Theme.backgroundAccentLarger
            case .rowSelected: Theme.backgroundAccentLargerStronger
            case .box: Theme.backgroundSecondary
            }
        case .secondary:
            switch layer {
            case .control, .row, .box, .chip: Theme.backgroundSecondaryAccent
            case .controlLit, .controlLitOnSelected, .rowSelected: Theme.backgroundSecondaryAccentStronger
            }
        case .card: Theme.cardAccent
        case .popover: Theme.popoverAccent
        case .composer: Theme.borderComposer
        case .glass: Theme.accentWithOpacityTint
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

private struct Box<S: InsettableShape>: ViewModifier {
    let shape: S
    let bordered: Bool
    @Environment(\.surface) private var surface

    func body(content: Content) -> some View {
        content
            .background(surface.color(.box), in: shape)
            .overlay {
                if bordered { shape.strokeBorder(surface.border, lineWidth: 1) }
            }
            .environment(\.surface, surface.inBox)
    }
}

extension View {
    /// Makes the view a box on the surface it is on, `bordered` in that surface's border.
    func box(in shape: some InsettableShape, bordered: Bool = false) -> some View {
        modifier(Box(shape: shape, bordered: bordered))
    }
}

#if os(iOS)
extension View {
    /// A sheet's content: on the background, with what lies on it filled from there, and closed by
    /// `closeModal`.
    func sheetSurface() -> some View {
        modifier(SheetSurface())
    }
}

private struct SheetSurface: ViewModifier {
    @Environment(\.dismiss) private var dismiss

    func body(content: Content) -> some View {
        content
            .presentationBackground(Color.themeBackground)
            .environment(\.surface, .background)
            .environment(\.closeModal, CloseModal { dismiss() })
    }
}
#endif

extension Color {
    static let themeForegroundOverPicture = Color(platform: Theme.foregroundOverPicture)
    static let themeMutedForegroundOverPicture = Color(platform: Theme.mutedForegroundOverPicture)
    static let themeControlOverPicture = Color(platform: Theme.controlOverPicture)
    static let themeControlLitOverPicture = Color(platform: Theme.controlLitOverPicture)
    static let themeScrim = Color(platform: Theme.scrim)
    static let themeGlassTint = Color(platform: Theme.glassTint)
}
