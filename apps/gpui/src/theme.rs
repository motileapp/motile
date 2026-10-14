//! Colours, type and sizes for the whole app, the same as the Mac app's `Theme.swift`. The
//! design tokens are generated into `tokens.rs` from `packages/theme/tokens.json`; `colors(cx)`
//! gives them in the appearance the window has now, with the colours made from them.

use gpui_kit::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

pub use crate::tokens::{Opacity, Radius, ShadowSize, Tokens};

/// The design tokens of one appearance, with the colours made from them. Every colour the app
/// shows is one of these; a view never makes a colour of its own.
#[derive(Clone, Copy)]
pub struct Colors {
    tokens: Tokens,
    pub dark: bool,
    /// The text colours as they are in the dark, for what lies over a picture.
    pub foreground_over_picture: Hsla,
    pub muted_foreground_over_picture: Hsla,
    /// A control as it is in the dark, over a picture: the background's accent, stronger when lit.
    pub control_over_picture: Hsla,
    pub control_lit_over_picture: Hsla,
    /// A colour's wash: the colour at color-tint. Chips, notices, diff lines.
    #[expect(dead_code, reason = "for the views still to be ported")]
    pub primary_tint: Hsla,
    pub primary_tint_stronger: Hsla,
    pub destructive_tint: Hsla,
    pub success_tint: Hsla,
    pub warning_tint: Hsla,
    /// What lies over the window behind a modal or the media viewer.
    pub scrim: Hsla,
    /// The colours code is highlighted with, by the core's palette index.
    pub syntax: [Hsla; 16],
}

impl std::ops::Deref for Colors {
    type Target = Tokens;

    fn deref(&self) -> &Tokens {
        &self.tokens
    }
}

impl Colors {
    fn new(tokens: Tokens, dark: bool) -> Self {
        let night = Tokens::dark();
        let thinned =
            |color: Hsla, opacity: Opacity| color.opacity(if dark { opacity.dark() } else { opacity.light() });
        let t = &tokens;
        Self {
            dark,
            foreground_over_picture: night.foreground,
            muted_foreground_over_picture: night.muted_foreground,
            control_over_picture: night.background_accent,
            control_lit_over_picture: night.background_accent_stronger,
            primary_tint: thinned(t.primary, Opacity::ColorTint),
            primary_tint_stronger: thinned(t.primary, Opacity::ColorTintStronger),
            destructive_tint: thinned(t.destructive, Opacity::ColorTint),
            success_tint: thinned(t.success, Opacity::ColorTint),
            warning_tint: thinned(t.warning, Opacity::ColorTint),
            scrim: thinned(t.overlay, Opacity::Overlay),
            syntax: [
                t.foreground,
                t.syntax_comment,
                t.syntax_keyword,
                t.syntax_string,
                t.syntax_constant,
                t.syntax_function,
                t.syntax_type,
                t.syntax_type,
                t.syntax_tag,
                t.syntax_constant,
                t.success,
                t.destructive,
                t.syntax_constant,
                t.syntax_constant,
                t.syntax_constant,
                t.syntax_comment,
            ],
            tokens,
        }
    }

    /// The opacity's value in this appearance.
    pub fn opacity(&self, opacity: Opacity) -> f32 {
        if self.dark { opacity.dark() } else { opacity.light() }
    }

    /// The colour at one of the opacities.
    pub fn at(&self, color: Hsla, opacity: Opacity) -> Hsla {
        color.opacity(self.opacity(opacity))
    }

    /// The colour's wash: at color-tint, or color-tint-stronger when lit.
    pub fn wash(&self, color: Hsla, lit: bool) -> Hsla {
        self.at(color, if lit { Opacity::ColorTintStronger } else { Opacity::ColorTint })
    }

    /// A shadow of the size, in the colour shadow at one of its three opacities. GPUI's blur is
    /// the Gaussian's spread, half of what CSS calls the blur.
    pub fn shadow(&self, size: ShadowSize, strength: Opacity) -> Vec<BoxShadow> {
        vec![BoxShadow {
            color: self.at(self.tokens.shadow, strength),
            offset: point(px(0.), px(size.down())),
            blur_radius: px(size.blur() / 2.),
            spread_radius: px(0.),
            inset: false,
        }]
    }
}

pub static LIGHT: std::sync::LazyLock<Colors> = std::sync::LazyLock::new(|| Colors::new(Tokens::light(), false));
pub static DARK: std::sync::LazyLock<Colors> = std::sync::LazyLock::new(|| Colors::new(Tokens::dark(), true));

/// What a view lies on: the page, a box on it, a card or a popover. What is drawn on a surface
/// takes the surface's accent.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Surface {
    #[default]
    Background,
    Secondary,
    Card,
    Popover,
}

/// What is drawn on a surface, each in the surface's own accent.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    /// A filled control at rest: a secondary button, a chip's fill, a track.
    Control,
    /// The same lit, or selected.
    ControlLit,
    /// A control lit on a selected row, which is already as light as a lit control.
    ControlLitOnSelected,
    /// A row or a tab lit.
    Row,
    /// A row or a tab selected.
    RowSelected,
    /// A box on it: code, a message, a notice, a quoted line.
    Box,
}

impl Surface {
    pub fn color(self, c: &Colors) -> Hsla {
        match self {
            Surface::Background => c.background,
            Surface::Secondary => c.background_secondary,
            Surface::Card => c.card,
            Surface::Popover => c.popover,
        }
    }

    /// The surface's edge, which the lines across it share.
    pub fn border(self, c: &Colors) -> Hsla {
        match self {
            Surface::Background | Surface::Secondary => c.border,
            Surface::Card => c.border_card,
            Surface::Popover => c.border_popover,
        }
    }

    pub fn layer(self, layer: Layer, c: &Colors) -> Hsla {
        match self {
            Surface::Background => match layer {
                Layer::Control => c.background_accent,
                Layer::ControlLit => c.background_accent_stronger,
                Layer::ControlLitOnSelected => c.background_accent_strongest,
                Layer::Row => c.background_accent_larger,
                Layer::RowSelected => c.background_accent_larger_stronger,
                Layer::Box => c.background_secondary,
            },
            Surface::Secondary => match layer {
                Layer::Control | Layer::Row | Layer::Box => c.background_secondary_accent,
                Layer::ControlLit | Layer::ControlLitOnSelected | Layer::RowSelected => {
                    c.background_secondary_accent_stronger
                }
            },
            Surface::Card => c.card_accent,
            Surface::Popover => c.popover_accent,
        }
    }

    /// What lies in a box on it.
    pub fn in_box(self) -> Surface {
        if self == Surface::Background { Surface::Secondary } else { self }
    }
}

/// How large a control is. Its height, symbol, text, padding and corners go together, so that
/// no view picks them one by one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ControlSize {
    /// Inside a row, a tab, a chip or a card.
    Small,
    /// Everywhere else.
    #[default]
    Regular,
    /// What a screen is about.
    Large,
}

impl ControlSize {
    pub const fn height(self) -> f32 {
        match self {
            ControlSize::Small => 24.,
            ControlSize::Regular => 28.,
            ControlSize::Large => 36.,
        }
    }

    pub fn symbol(self) -> f32 {
        match self {
            ControlSize::Small => 13.,
            ControlSize::Regular => 14.,
            ControlSize::Large => 16.,
        }
    }

    /// A symbol that should weigh less than the others, like the x that closes a tab.
    pub fn small_symbol(self) -> f32 {
        self.symbol() - 2.
    }

    pub fn text_size(self) -> f32 {
        match self {
            ControlSize::Small => 11.5,
            ControlSize::Regular => 12.,
            ControlSize::Large => 13.,
        }
    }

    /// The room between what it says and its sides.
    pub fn padding(self) -> f32 {
        match self {
            ControlSize::Small => 8.,
            ControlSize::Regular => 11.,
            ControlSize::Large => 14.,
        }
    }

    /// How much nearer its side a chevron stands than words do, so that the control's sides look alike.
    pub fn chevron_outset(self) -> f32 {
        if self == ControlSize::Regular { 2. } else { 0. }
    }

    /// The room between its symbol and its words.
    pub fn gap(self) -> f32 {
        match self {
            ControlSize::Small => 5.,
            ControlSize::Regular => 6.,
            ControlSize::Large => 8.,
        }
    }

    pub fn radius(self) -> f32 {
        match self {
            ControlSize::Small => Radius::SM,
            ControlSize::Regular | ControlSize::Large => Radius::MD,
        }
    }

    /// The side of the square its symbol is drawn in.
    pub fn symbol_side(self) -> f32 {
        symbol_side(self.symbol())
    }

    /// How much nearer its side a symbol stands than words do, so that both look as far from it.
    pub fn symbol_outset(self) -> f32 {
        (self.symbol_side() / 5. * 2.).round() / 2.
    }
}

/// How much larger than the text beside it an icon's square is.
pub const SYMBOL_SCALE: f32 = 1.2;

/// The side of the square a symbol for text of `size` is drawn in.
pub fn symbol_side(size: f32) -> f32 {
    (size * SYMBOL_SCALE).round()
}

/// Whether the app is drawn dark now: the setting, or the system's appearance under "System".
#[derive(Default)]
pub struct Mode {
    pub dark: bool,
}

impl Global for Mode {}

pub fn is_dark(cx: &App) -> bool {
    cx.try_global::<Mode>().is_some_and(|mode| mode.dark)
}

pub fn colors(cx: &App) -> &'static Colors {
    if is_dark(cx) { &DARK } else { &LIGHT }
}

pub fn appearance_is_dark(appearance: WindowAppearance) -> bool {
    matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark)
}

/// Sets the app's mode and has the components of GPUI Kit follow it in the same colours.
pub fn apply(dark: bool, window: &mut Window, cx: &mut App) {
    cx.set_global(Mode { dark });
    let mode = if dark { component::ThemeMode::Dark } else { component::ThemeMode::Light };
    component::Theme::change(mode, Some(window), cx);
    let c = colors(cx);
    component::Theme::update(cx, |theme| {
        theme.font_family = UI_FONT.into();
        theme.mono_font_family = MONO_FONT.into();
        theme.font_size = px(13.);
        theme.mono_font_size = px(CODE_SIZE);
        theme.background = c.background;
        theme.foreground = c.foreground;
        theme.border = c.border;
        theme.input = c.border_input;
        theme.popover = c.popover;
        theme.popover_foreground = c.foreground;
        theme.muted = c.background_secondary;
        theme.muted_foreground = c.muted_stronger_foreground;
        theme.accent = c.popover_accent;
        theme.accent_foreground = c.foreground;
        theme.list_hover = c.background_accent_larger;
        theme.list_active = c.background_accent_larger_stronger;
        theme.primary = c.primary;
        theme.primary_foreground = c.primary_foreground;
        theme.ring = c.ring;
        theme.caret = c.foreground;
        theme.selection = c.primary_tint_stronger;
        theme.link = c.primary;
        theme.danger = c.destructive;
        theme.danger_foreground = c.destructive_foreground;
        theme.radius = px(Radius::MD);
        theme.radius_lg = px(Radius::LG);
        theme.shadow = true;
    });
}

/// Has the whole app, its title bars and menus too, take the appearance picked in Settings.
#[cfg(target_os = "macos")]
pub fn set_app_appearance(appearance: Appearance) {
    use objc2_app_kit::{NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication};
    let Some(main) = objc2::MainThreadMarker::new() else { return };
    let named = match appearance {
        Appearance::System => None,
        Appearance::Light => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameAqua }),
        Appearance::Dark => NSAppearance::appearanceNamed(unsafe { NSAppearanceNameDarkAqua }),
    };
    NSApplication::sharedApplication(main).setAppearance(named.as_deref());
}

#[cfg(not(target_os = "macos"))]
pub fn set_app_appearance(_: Appearance) {}

/// The app's typeface, bundled in `assets/fonts`.
pub const UI_FONT: &str = "DM Sans";
#[cfg(target_os = "macos")]
pub const MONO_FONT: &str = ".AppleSystemUIFontMonospaced";
#[cfg(windows)]
pub const MONO_FONT: &str = "Consolas";
#[cfg(not(any(target_os = "macos", windows)))]
pub const MONO_FONT: &str = "DejaVu Sans Mono";
/// The system's own typeface, which the Mac draws numbers in where it says so.
#[cfg(target_os = "macos")]
pub const SYSTEM_FONT: &str = ".AppleSystemUIFont";
#[cfg(not(target_os = "macos"))]
pub const SYSTEM_FONT: &str = UI_FONT;

pub const PROSE_SIZE: f32 = 14.;
pub const CODE_SIZE: f32 = 12.5;
pub const PROSE_LINE_HEIGHT: f32 = 24.;
pub const CODE_LINE_HEIGHT: f32 = 18.;

pub fn heading_size(level: u8) -> f32 {
    match level {
        1 => 20.,
        2 => 17.,
        3 => 15.,
        _ => 14.,
    }
}

/// The widest the transcript gets.
pub const CONTENT_WIDTH: f32 = 768.;
pub const CONTENT_PADDING: f32 = 38.;
/// How far the composer reaches past the transcript on each side.
pub const COMPOSER_REACH: f32 = 14.;
pub const COMPOSER_WIDTH: f32 = CONTENT_WIDTH + COMPOSER_REACH * 2.;
pub const COMPOSER_PADDING: f32 = CONTENT_PADDING - COMPOSER_REACH;
/// The invisible area that takes the drag around a line that resizes.
pub const RESIZE_GRAB: f32 = 17.;
/// The height of the window's top bar, where the window's buttons are.
pub const TOP_BAR: f32 = 52.;
