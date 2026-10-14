//! What every settings page is made of: groups under a title, rows of a label and controls, notes,
//! the menu of a row's other actions, and the marks a search scrolls to.

use std::collections::HashMap;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, Surface, colors};
use crate::ui::menu::Menu;
use crate::ui::{ActionMenu, card_default, icons};

/// How far a group's rows stand from the card's sides.
pub const INSET: f32 = 14.;
/// How tall a row is at least.
pub const ROW_HEIGHT: f32 = 48.;

/// What stands a control this tall as far from its row's right as from its top and bottom.
pub fn row_outset(height: f32) -> f32 {
    INSET - (ROW_HEIGHT - height) / 2.
}

/// How far a large control is pulled past the row's inset.
pub fn large_outset() -> f32 {
    row_outset(ControlSize::Large.height())
}

/// Where the groups and rows a search can scroll to were drawn last, by their ids.
#[derive(Default)]
pub struct Targets(HashMap<String, Bounds<Pixels>>);

impl Global for Targets {}

impl Targets {
    /// Forgets what was drawn before, as a page that is drawn again marks its own.
    pub fn clear(cx: &mut App) {
        cx.default_global::<Targets>().0.clear();
    }

    pub fn bounds(id: &str, cx: &App) -> Option<Bounds<Pixels>> {
        cx.try_global::<Targets>()?.0.get(id).copied()
    }
}

/// Fills its parent, which makes the parent the place a search for `id` scrolls to.
pub fn target(id: impl Into<String>) -> impl IntoElement {
    let id = id.into();
    canvas(
        move |bounds, _, cx| {
            cx.default_global::<Targets>().0.insert(id.clone(), bounds);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

/// A title, a caption under it when it has one, and its rows in one card, or each in its own
/// card when it isn't `carded`.
pub fn group(
    id: impl Into<String>,
    title: impl Into<SharedString>,
    caption: Option<&str>,
    carded: bool,
    rows: Vec<AnyElement>,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    let rows = if carded {
        card_default(div().flex().flex_col(), cx).children(rows)
    } else {
        div().flex().flex_col().gap(px(12.)).children(rows)
    };
    div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(target(id))
        .child(
            div()
                .px(px(4.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).text_color(c.foreground).child(title.into()))
                .when_some(caption, |heading, caption| {
                    heading.child(
                        div().text_size(px(11.5)).text_color(c.muted_stronger_foreground).child(caption.to_string()),
                    )
                }),
        )
        .child(rows)
        .into_any_element()
}

/// What the row is about on the left, its controls on the right, a large one as far from the
/// row's right as from its top and bottom.
pub fn row(leading: Vec<AnyElement>, trailing: Vec<AnyElement>) -> Div {
    div()
        .px(px(INSET))
        .min_h(px(ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().flex_1().min_w_0().pr(px(22.)).flex().items_center().gap(px(10.)).children({
            let last = leading.len().saturating_sub(1);
            leading.into_iter().enumerate().map(move |(index, part)| {
                div()
                    .py(px(8.))
                    .flex()
                    .map(|cell| if index == last { cell.flex_1().min_w_0() } else { cell.flex_shrink_0() })
                    .child(part)
            })
        }))
        .when(!trailing.is_empty(), |row| {
            row.child(
                div().flex_shrink_0().mr(px(-large_outset())).flex().items_center().gap(px(10.)).children(trailing),
            )
        })
}

/// How a description that doesn't fit its line is cut, where it isn't wrapped: a path at its
/// start, an address in its middle, a sentence at its end. The title above it always wraps.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    Wraps,
    Start,
    Middle,
    End,
}

/// What a row is about: its name, and under it what it does.
pub fn label(
    title: impl Into<SharedString>,
    description: Option<String>,
    icon: Option<&'static str>,
    cut: Cut,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(8.))
        .when_some(icon, |label, icon| {
            label.child(div().flex_shrink_0().text_color(c.muted_foreground).child(icons::symbol(icon, 13.)))
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(title.into()))
                .when_some(description, |label, description| {
                    label.child(
                        div()
                            .text_size(px(11.5))
                            .text_color(c.muted_foreground)
                            .when(cut != Cut::Wraps, |text| text.whitespace_nowrap().overflow_hidden())
                            .map(|text| match cut {
                                Cut::Wraps => text,
                                Cut::Start => text.text_ellipsis_start(),
                                Cut::Middle => text.text_ellipsis_middle(),
                                Cut::End => text.text_ellipsis(),
                            })
                            .child(description),
                    )
                }),
        )
        .into_any_element()
}

/// Said on a page with nothing to set yet.
pub fn note(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    let c = colors(cx);
    card_default(div().w_full().p(px(INSET)).text_size(px(13.)).text_color(c.muted_foreground).child(text.into()), cx)
        .into_any_element()
}

/// The row's other actions, under its three dots.
pub fn more(
    id: impl Into<ElementId>,
    help: impl Into<SharedString>,
    build: impl Fn(&mut Window, &mut App) -> Menu + 'static,
) -> ActionMenu {
    ActionMenu::icon(id, "ellipsis", help, build)
        .button(|button| button.size(ControlSize::Large).surface(Surface::Card))
}
