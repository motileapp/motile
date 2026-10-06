//! Colours, type and sizes for the whole app, the same as the Mac app's `Theme.swift`. Each
//! colour has a light and a dark value; `colors(cx)` gives the ones of the appearance the window
//! has now.

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

#[derive(Clone, Copy)]
pub struct Colors {
    // Surfaces
    pub background: Hsla,
    pub background_secondary: Hsla,
    pub background_tertiary: Hsla,
    pub background_quaternary: Hsla,
    pub popover: Hsla,
    pub popover_secondary: Hsla,
    pub composer: Hsla,
    pub composer_secondary: Hsla,
    /// Borders are solid, so that where two meet they do not darken.
    pub border: Hsla,
    pub border_secondary: Hsla,

    // Text
    pub text: Hsla,
    pub prose: Hsla,
    pub secondary: Hsla,
    pub tertiary: Hsla,
    pub activity: Hsla,
    pub shimmer: Hsla,

    // Meaning
    pub primary: Hsla,
    pub primary_hover: Hsla,
    pub link: Hsla,
    pub link_hover: Hsla,
    pub danger: Hsla,
    pub danger_fill: Hsla,
    pub danger_background: Hsla,
    pub warning: Hsla,
    pub warning_background: Hsla,
    pub success: Hsla,
    pub merged: Hsla,
    pub working: Hsla,
    pub unread: Hsla,

    // Charts: what tells one agent's line from the other's.
    pub claude_series: Hsla,
    pub codex_series: Hsla,

    /// What lies over the window behind a sheet or the media viewer.
    pub scrim: Hsla,

    /// The colours code is highlighted with, by the core's palette index.
    pub syntax: [Hsla; 16],
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

fn hexa(value: u32, alpha: f32) -> Hsla {
    let mut color: Hsla = rgb(value).into();
    color.a = alpha;
    color
}

fn black(alpha: f32) -> Hsla {
    hsla(0., 0., 0., alpha)
}

pub static LIGHT: std::sync::LazyLock<Colors> = std::sync::LazyLock::new(|| {
    let text = hex(0x22242b);
    Colors {
        background: hex(0xf8f9fc),
        background_secondary: hex(0xeceef4),
        background_tertiary: hex(0xe1e4ed),
        background_quaternary: hex(0xd6dae6),
        popover: hex(0xffffff),
        popover_secondary: hex(0xeceef4),
        composer: hex(0xffffff),
        composer_secondary: hex(0xeceef4),
        border: hex(0xe2e3e5),
        border_secondary: hex(0xd5d6d9),
        text,
        prose: hex(0x383b45),
        secondary: hex(0x6b6f7c),
        tertiary: hex(0x9a9eab),
        activity: hex(0x6b6f7c),
        shimmer: hex(0x000000),
        primary: hex(0x2a5bd7),
        primary_hover: hexa(0x2a5bd7, 0.1),
        link: hex(0x1d4ed8),
        link_hover: hexa(0x1d4ed8, 0.12),
        danger: hex(0xc62828),
        danger_fill: hex(0xc62828),
        danger_background: hexa(0xdc2626, 0.07),
        warning: hex(0xb45309),
        warning_background: hexa(0xf59e0b, 0.1),
        success: hex(0x047857),
        merged: hex(0x8250df),
        working: hex(0x0284c7),
        unread: hex(0xea580c),
        claude_series: hex(0xeb6834),
        codex_series: hex(0x2a78d6),
        scrim: black(0.32),
        syntax: [
            text,
            hex(0x6e7781),
            hex(0xcf222e),
            hex(0x0a3069),
            hex(0x0550ae),
            hex(0x8250df),
            hex(0x953800),
            hex(0x953800),
            hex(0x116329),
            hex(0x0550ae),
            hex(0x116329),
            hex(0xb3261e),
            hex(0x0550ae),
            hex(0x0550ae),
            hex(0x0550ae),
            hex(0x6e7781),
        ],
    }
});

pub static DARK: std::sync::LazyLock<Colors> = std::sync::LazyLock::new(|| {
    let text = hex(0xdcdee4);
    Colors {
        background: hex(0x0a0b0f),
        background_secondary: hex(0x111217),
        background_tertiary: hex(0x191a1f),
        background_quaternary: hex(0x212227),
        popover: hex(0x191a1f),
        popover_secondary: hex(0x222226),
        composer: hex(0x111217),
        composer_secondary: hex(0x191a1f),
        border: hex(0x191a1e),
        border_secondary: hex(0x2b2b2f),
        text,
        prose: hex(0xa8abb6),
        secondary: hex(0x9a9eab),
        tertiary: hex(0x646875),
        activity: hex(0x7a7e8b),
        shimmer: hex(0xffffff),
        primary: hex(0x4f7cff),
        primary_hover: hexa(0x4f7cff, 0.18),
        link: hex(0x7aa2ff),
        link_hover: hexa(0x7aa2ff, 0.12),
        danger: hex(0xff7b72),
        danger_fill: hex(0xe5484d),
        danger_background: hexa(0xff5c5c, 0.1),
        warning: hex(0xf5b454),
        warning_background: hexa(0xf59e0b, 0.12),
        success: hex(0x55c483),
        merged: hex(0xba93fb),
        working: hex(0x38bdf8),
        unread: hex(0xfb923c),
        claude_series: hex(0xd95926),
        codex_series: hex(0x3987e5),
        scrim: black(0.5),
        syntax: [
            text,
            hex(0x8b949e),
            hex(0xff7b72),
            hex(0xa5d6ff),
            hex(0x79c0ff),
            hex(0xd2a8ff),
            hex(0xffa657),
            hex(0xffa657),
            hex(0x7ee787),
            hex(0x79c0ff),
            hex(0x7ee787),
            hex(0xffa198),
            hex(0x79c0ff),
            hex(0x79c0ff),
            hex(0x79c0ff),
            hex(0x8b949e),
        ],
    }
});

/// What a view lies on. The page is a ladder: what lies on one layer is filled with the next, and
/// so is what the pointer is over. What floats over the page, a popover or the composer, has one
/// colour of its own for both.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Surface {
    #[default]
    Background,
    Secondary,
    Tertiary,
    Quaternary,
    Popover,
    PopoverSecondary,
    Composer,
    ComposerSecondary,
}

impl Surface {
    /// What lies on it, and what the pointer is over.
    pub fn next(self) -> Surface {
        match self {
            Surface::Background => Surface::Secondary,
            Surface::Secondary => Surface::Tertiary,
            Surface::Tertiary => Surface::Quaternary,
            Surface::Quaternary => Surface::Tertiary,
            Surface::Popover => Surface::PopoverSecondary,
            Surface::PopoverSecondary => Surface::Popover,
            Surface::Composer => Surface::ComposerSecondary,
            Surface::ComposerSecondary => Surface::Composer,
        }
    }

    /// What is selected, and a filled control under the pointer.
    pub fn further(self) -> Surface {
        match self {
            Surface::Background => Surface::Tertiary,
            Surface::Secondary
            | Surface::Popover
            | Surface::PopoverSecondary
            | Surface::Composer
            | Surface::ComposerSecondary => Surface::Background,
            Surface::Tertiary | Surface::Quaternary => Surface::Secondary,
        }
    }

    pub fn color(self, c: &Colors) -> Hsla {
        match self {
            Surface::Background => c.background,
            Surface::Secondary => c.background_secondary,
            Surface::Tertiary => c.background_tertiary,
            Surface::Quaternary => c.background_quaternary,
            Surface::Popover => c.popover,
            Surface::PopoverSecondary => c.popover_secondary,
            Surface::Composer => c.composer,
            Surface::ComposerSecondary => c.composer_secondary,
        }
    }

    /// The border around a card or a field that lies on it.
    pub fn border(self, c: &Colors) -> Hsla {
        match self {
            Surface::Background => c.border,
            _ => c.border_secondary,
        }
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
    pub fn height(self) -> f32 {
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
            ControlSize::Small => Radius::SMALL,
            ControlSize::Regular => Radius::CONTROL,
            ControlSize::Large => Radius::LARGE,
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

/// How round corners are.
pub struct Radius;

impl Radius {
    pub const SMALL: f32 = 6.;
    pub const CONTROL: f32 = 7.;
    pub const LARGE: f32 = 9.;
    /// Boxes that hold text or rows: cards, code, notices.
    pub const CARD: f32 = 10.;
    pub const SHEET: f32 = 14.;
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
        theme.foreground = c.text;
        theme.border = c.border;
        theme.input = c.border;
        theme.popover = c.popover;
        theme.popover_foreground = c.text;
        theme.muted = c.background_secondary;
        theme.muted_foreground = c.tertiary;
        theme.accent = c.background_secondary;
        theme.accent_foreground = c.text;
        theme.list_hover = c.background_secondary;
        theme.list_active = c.background_tertiary;
        theme.primary = c.primary;
        theme.ring = c.primary;
        theme.caret = c.text;
        theme.selection = c.primary_hover;
        theme.link = c.link;
        theme.danger = c.danger;
        theme.radius = px(Radius::CONTROL);
        theme.radius_lg = px(Radius::CARD);
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
#[cfg(not(target_os = "macos"))]
pub const MONO_FONT: &str = "DejaVu Sans Mono";

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
