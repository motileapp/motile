//! Colours and type for the whole app, the same as the Mac app's. Each colour has a light and a
//! dark value; `colors(cx)` gives the ones of the appearance the window has now.

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
    pub background: Hsla,
    pub raised: Hsla,
    /// The composer floats over the transcript, so it is opaque: nothing blurs what is under it.
    pub composer: Hsla,
    pub bubble: Hsla,
    pub code_background: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub border: Hsla,
    pub strong_border: Hsla,

    pub text: Hsla,
    pub prose: Hsla,
    pub secondary: Hsla,
    pub tertiary: Hsla,
    /// The system's label colours, which SwiftUI's `.secondary` and `.tertiary` are: the text's
    /// colour, lighter.
    pub label_secondary: Hsla,
    pub label_tertiary: Hsla,

    pub primary: Hsla,
    pub primary_hover: Hsla,
    /// The system's accent colour, which the Mac fills its prominent buttons with.
    pub accent: Hsla,
    pub link: Hsla,
    pub link_hover: Hsla,
    pub danger: Hsla,
    pub danger_background: Hsla,
    pub warning: Hsla,
    pub warning_background: Hsla,
    pub success: Hsla,
    pub working: Hsla,
    pub unread: Hsla,
    /// The surface of a popover.
    pub popover: Hsla,
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

fn white(alpha: f32) -> Hsla {
    hsla(0., 0., 1., alpha)
}

fn black(alpha: f32) -> Hsla {
    hsla(0., 0., 0., alpha)
}

// The dark surfaces stay clear of gray level 16, where some monitors flicker.
pub static LIGHT: std::sync::LazyLock<Colors> = std::sync::LazyLock::new(|| {
    let text = hex(0x27272a);
    Colors {
        background: hex(0xfcfcfc),
        raised: hex(0xffffff),
        composer: hex(0xffffff),
        bubble: hex(0xf1f1f3),
        code_background: hex(0xf6f6f7),
        hover: black(0.045),
        selected: black(0.08),
        border: black(0.09),
        strong_border: black(0.14),
        text,
        prose: hex(0x3a3a40),
        secondary: hex(0x71717a),
        tertiary: hex(0xa1a1aa),
        label_secondary: black(0.5),
        label_tertiary: black(0.26),
        primary: hex(0x2a5bd7),
        primary_hover: hexa(0x2a5bd7, 0.1),
        accent: hex(0x007aff),
        link: hex(0x0068da),
        link_hover: hexa(0x0068da, 0.12),
        danger: hex(0xc62828),
        danger_background: hexa(0xdc2626, 0.07),
        warning: hex(0xb45309),
        warning_background: hexa(0xf59e0b, 0.1),
        success: hex(0x047857),
        working: hex(0x0284c7),
        unread: hex(0xea580c),
        popover: hex(0xe8e8e8),
        scrim: black(0.2),
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
    let text = hex(0xececee);
    Colors {
        background: hex(0x19191a),
        raised: hex(0x242426),
        composer: hex(0x222223),
        bubble: hex(0x2d2d30),
        code_background: hex(0x222224),
        hover: white(0.06),
        selected: white(0.1),
        border: white(0.09),
        strong_border: white(0.14),
        text,
        prose: hex(0xc2c2c7),
        secondary: hex(0x9c9ca6),
        tertiary: hex(0x6c6c75),
        label_secondary: white(0.55),
        label_tertiary: white(0.25),
        primary: hex(0x4f7cff),
        primary_hover: hexa(0x4f7cff, 0.18),
        accent: hex(0x0a84ff),
        link: hex(0x419cff),
        link_hover: hexa(0x419cff, 0.12),
        danger: hex(0xff7b72),
        danger_background: hexa(0xff5c5c, 0.1),
        warning: hex(0xf5b454),
        warning_background: hexa(0xf59e0b, 0.12),
        success: hex(0x4ade80),
        working: hex(0x38bdf8),
        unread: hex(0xfb923c),
        popover: hex(0x2c2c2e),
        scrim: black(0.45),
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
        theme.font_family = SYSTEM_FONT.into();
        theme.mono_font_family = MONO_FONT.into();
        theme.font_size = px(13.);
        theme.mono_font_size = px(CODE_SIZE);
        theme.background = c.background;
        theme.foreground = c.text;
        theme.border = c.border;
        theme.input = c.strong_border;
        theme.popover = c.raised;
        theme.popover_foreground = c.text;
        theme.muted = c.hover;
        theme.muted_foreground = c.label_tertiary;
        theme.accent = c.hover;
        theme.accent_foreground = c.text;
        theme.list_hover = c.hover;
        theme.list_active = c.selected;
        theme.primary = c.primary;
        theme.ring = c.primary;
        theme.caret = c.text;
        theme.selection = c.primary_hover;
        theme.link = c.link;
        theme.danger = c.danger;
        theme.radius = px(6.);
        theme.radius_lg = px(10.);
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

pub const SYSTEM_FONT: &str = ".SystemUIFont";
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

/// The widest the transcript and the composer get.
pub const CONTENT_WIDTH: f32 = 768.;
pub const CONTENT_PADDING: f32 = 24.;
/// The invisible area that takes the drag around a line that resizes.
pub const RESIZE_GRAB: f32 = 17.;
/// The height of the window's top bar, where the window's buttons are.
pub const TOP_BAR: f32 = 52.;
