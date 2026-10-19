//! The program's colours as the clients draw them (`style.ts`'s `fg`, `bg` and `onColor`): tokens
//! of `tokens.json`, the surface's own colours, or a CSS colour as an RGBA, with the ink that reads
//! on it.

use std::sync::LazyLock;

use regress::Regex;

use super::super::canvas::svg::openui_color;
use super::{Color, Layer};

/// What a raw background makes the text on it: dark or light.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ink {
    Dark,
    Light,
}

impl Ink {
    /// The ink at a share of its strength: 1 for `ink`, 0.72 for `muted`, 0.55 for `faint`.
    pub fn at(self, alpha: f32) -> Color {
        let level = match self {
            Ink::Dark => 13,
            Ink::Light => 255,
        };
        Color::Rgba([level, level, level, (alpha * 255.0).round() as u8])
    }
}

const STATUS: [(&str, &str); 6] = [
    ("info", "foreground"),
    ("success", "success"),
    ("warning", "warning"),
    ("danger", "destructive"),
    ("purple", "merged"),
    ("pink", "pink"),
];

/// A model-written value that could load something: `url(...)`.
static UNSAFE_CSS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::with_flags(
        r"(url|image|image-set|cross-fade|element|expression|paint|var|attr|env)\s*\(|@import|[\\;{}<>]",
        "i",
    )
    .expect("a valid pattern")
});
static HEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"^#([\da-f]{3}|[\da-f]{6})$", "i").expect("a valid pattern"));
static RGB: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)", "i").expect("a valid pattern"));

pub fn token(name: &'static str) -> Color {
    Color::token(name)
}

/// The text colour, the ink on a raw background or `foreground`.
pub fn ink(ink: Option<Ink>) -> Color {
    ink.map_or_else(|| token("foreground"), |ink| ink.at(1.0))
}

pub fn muted(ink: Option<Ink>) -> Color {
    ink.map_or_else(|| token("muted-foreground"), |ink| ink.at(0.72))
}

pub fn faint(ink: Option<Ink>) -> Color {
    ink.map_or_else(|| token("muted-stronger-foreground"), |ink| ink.at(0.55))
}

/// The colour of text, an icon or a stroke.
pub fn foreground(value: &str, on: Option<Ink>) -> Option<Color> {
    match value {
        "ink" => Some(ink(on)),
        "muted" => Some(muted(on)),
        "faint" => Some(faint(on)),
        value => openui_color(value).or_else(|| css(value)),
    }
}

/// The colour of a background: status colours are their washes.
pub fn background(value: &str, on: Option<Ink>) -> Option<Color> {
    if let Some((_, name)) = STATUS.iter().find(|(key, _)| *key == value) {
        return Some(Color::Wash((*name).into()));
    }
    match value {
        "surface" => Some(Color::Surface(Layer::Box)),
        "surface-strong" => Some(Color::Surface(Layer::Control)),
        "border" => Some(Color::Surface(Layer::Border)),
        value => foreground(value, on),
    }
}

/// Whether a background is one of the theme's, whose content lies on its inner surface.
pub fn is_theme(color: &Color) -> bool {
    !matches!(color, Color::Rgba(_))
}

/// A CSS colour the browser would draw.
fn css(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.is_empty() || UNSAFE_CSS.find(value).is_some() {
        return None;
    }
    let color = csscolorparser::parse(value).ok()?;
    Some(Color::Rgba(color.to_rgba8()))
}

/// The ink that reads on a raw background written as hex or `rgb()`, by its luminance.
pub fn ink_on(background: &str) -> Option<Ink> {
    let tokens = ["surface", "surface-strong", "border", "ink", "muted", "faint", "canvas"];
    if tokens.contains(&background) || openui_color(background).is_some() {
        return None;
    }
    let value = background.trim();
    let channels: Vec<u32> = if let Some(found) = HEX.find(value) {
        let digits = &value[found.group(1)?];
        let digits: String = match digits.len() {
            3 => digits.chars().flat_map(|digit| [digit, digit]).collect(),
            _ => digits.to_owned(),
        };
        (0..3).map(|index| u32::from_str_radix(&digits[index * 2..index * 2 + 2], 16).unwrap_or(0)).collect()
    } else {
        let found = RGB.find(value)?;
        (1..=3).map(|group| found.group(group).map_or(0, |range| value[range].parse().unwrap_or(u32::MAX))).collect()
    };
    let linear = |channel: u32| {
        let value = channel as f64 / 255.0;
        if value <= 0.03928 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    let luminance = 0.2126 * linear(channels[0]) + 0.7152 * linear(channels[1]) + 0.0722 * linear(channels[2]);
    Some(if luminance > 0.36 { Ink::Dark } else { Ink::Light })
}
