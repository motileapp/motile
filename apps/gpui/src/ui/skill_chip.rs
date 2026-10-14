//! A skill a message names, drawn as a chip in the composer and in the transcript: the skill's
//! symbol and its name in the skill colour, on its wash, as the Mac app's `SkillChip` draws it.

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{self, Opacity, Radius, colors};
use crate::ui::icons;

pub const SYMBOL: &str = "box";
const SYMBOL_SIZE: f32 = 11.;
const PADDING: f32 = 5.;
const GAP: f32 = 4.;
/// The room between the chip and the text on either side.
pub const MARGIN: f32 = 1.;
pub const TEXT_SIZE: f32 = 13.;
/// As tall as a line of the composer's text.
const HEIGHT: f32 = 20.;

/// The room before the name: the padding, the symbol and the gap after it.
fn lead() -> f32 {
    PADDING + theme::symbol_side(SYMBOL_SIZE) + GAP
}

/// The room a message leaves before and after a chip's name.
pub fn room() -> (f32, f32) {
    (MARGIN + lead(), PADDING + MARGIN)
}

/// The chip in the composer's text, `height` tall.
pub fn chip(label: SharedString, selected: bool, height: Pixels, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .mx(px(MARGIN))
        .h(height)
        .flex()
        .items_center()
        .gap(px(GAP))
        .px(px(PADDING))
        .rounded(px(Radius::XS))
        .border_1()
        .border_color(c.at(c.skill, Opacity::ColorTintChart))
        .bg(c.wash(c.skill, selected))
        .child(icons::symbol(SYMBOL, SYMBOL_SIZE).text_color(c.skill))
        .child(
            div()
                .text_size(px(TEXT_SIZE))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.skill)
                .whitespace_nowrap()
                .child(label),
        )
}

/// The chip's box and symbol behind a message's name of a skill, which runs from `start` to
/// `end` on a line `line_height` tall whose top is at `start.y`.
pub fn paint_box(start: Point<Pixels>, end: Point<Pixels>, line_height: Pixels, window: &mut Window, cx: &App) {
    let c = colors(cx);
    let height = px(HEIGHT);
    let top = start.y + (line_height - height) / 2.;
    let bounds = Bounds::from_corners(point(start.x - px(lead()), top), point(end.x + px(PADDING), top + height));
    let edge = c.at(c.skill, Opacity::ColorTintChart);
    window.paint_quad(quad(bounds, px(Radius::XS), c.wash(c.skill, false), px(1.), edge, BorderStyle::Solid));
    let side = px(theme::symbol_side(SYMBOL_SIZE));
    let symbol = Bounds::new(point(bounds.left() + px(PADDING), top + (height - side) / 2.), size(side, side));
    let _ = window.paint_svg(symbol, icons::path(SYMBOL), None, TransformationMatrix::unit(), c.skill, cx);
}
