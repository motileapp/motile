//! How every list a dropdown draws is set, as the Apple kit's `DropdownList`: rows as tall as a
//! regular control, the same room around them, and a search over them whose symbol and words line
//! up with the rows'.

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, IconSize, Radius, Surface, colors};
use crate::ui::{Highlight, edges, icons};

/// The room between the card's edge and its rows.
pub const PADDING: f32 = 4.;
pub const ROW_HEIGHT: f32 = ControlSize::Regular.height();
/// The room between a row's side and what it shows.
pub const ROW_PADDING: f32 = 8.;
/// The room between a row's symbol and its words.
pub const GAP: f32 = 8.;
pub const TEXT_SIZE: f32 = 13.;
pub const SYMBOL: IconSize = IconSize::Md;
pub const CHECK: IconSize = IconSize::Sm;
/// The room above and below a row with a line under its title.
pub const DETAIL_PADDING: f32 = 6.;
/// The search over the list stands where a row would, its symbol over the rows' symbols.
pub const SEARCH_HEIGHT: f32 = ROW_HEIGHT + 2. * PADDING;
pub const SEARCH_PADDING: f32 = PADDING + ROW_PADDING;
/// The room between two rows' lights: only drawn, so that the rows' presses meet.
pub const LIGHT_INSET: f32 = 1.;

/// A row of a dropdown's list, which `light` lights.
pub fn row(id: impl Into<ElementId>, group: impl Into<SharedString>, cx: &App) -> Stateful<Div> {
    div()
        .id(id)
        .group(group)
        .relative()
        .flex_shrink_0()
        .min_h(px(ROW_HEIGHT))
        .px(px(ROW_PADDING))
        .flex()
        .items_center()
        .gap(px(GAP))
        .text_size(px(TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(colors(cx).foreground)
}

/// The light of a row, under the pointer or the arrow keys when `lit`.
pub fn light(group: impl Into<SharedString>, lit: bool, cx: &App) -> Div {
    Highlight::new()
        .radius(Radius::SM)
        .inset(edges(LIGHT_INSET, 0., LIGHT_INSET, 0.))
        .surface(Surface::Popover)
        .lit(lit)
        .render(group, cx)
}

/// The check at a row's end, there only while the row is the one chosen.
pub fn check(chosen: bool) -> Div {
    div().relative().flex_shrink_0().flex().when(!chosen, |mark| mark.opacity(0.)).child(icons::symbol("check", CHECK))
}

/// What a group of rows is, and what it is about at its end.
pub fn title(title: impl Into<SharedString>, about: Option<AnyElement>, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .flex_shrink_0()
        .px(px(ROW_PADDING))
        .pt(px(8.))
        .pb(px(4.))
        .flex()
        .items_center()
        .gap(px(GAP))
        .text_size(px(ControlSize::Regular.text_size()))
        .font_weight(FontWeight::MEDIUM)
        .child(div().text_color(c.muted_foreground).child(title.into()))
        .when_some(about, |row, about| row.child(div().flex_1()).child(about))
}

/// The search over a list, which takes the keys when pressed anywhere.
pub fn search(id: impl Into<ElementId>, query: &Entity<InputState>, cx: &App) -> Stateful<Div> {
    let focus = query.clone();
    div()
        .id(id)
        .h(px(SEARCH_HEIGHT))
        .flex_shrink_0()
        .px(px(SEARCH_PADDING))
        .flex()
        .items_center()
        .gap(px(GAP))
        .cursor_text()
        .on_click(move |_, window, cx| focus.update(cx, |query, cx| query.focus(window, cx)))
        .child(icons::symbol("search", SYMBOL).text_color(colors(cx).muted_stronger_foreground))
        .child(div().flex_1().child(Input::new(query).appearance(false).px_0().py_0().text_size(px(TEXT_SIZE))))
}
