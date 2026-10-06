//! The small pieces every screen is made of: the light under the pointer, buttons that are only
//! a symbol or only their text, and the icons of agents and projects.

pub mod alert;
pub mod control;
pub mod icons;
pub mod logos;
pub mod menu;
pub mod rich_text;
pub mod sheet;

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::colors;
pub use control::{
    ActionButton, ActionMenu, Chip, InputField, InputVariant, Segmented, Spinner, Switch, Variant, card, card_default,
};

pub type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The light under what the pointer is over, and under what is selected. It is drawn `inset`
/// from the edges of the element `group` names: that margin looks empty but is the element's,
/// so neighbours leave no gap to miss.
pub fn highlight(group: impl Into<SharedString>, radius: f32, inset: Edges<f32>, selected: bool, cx: &App) -> Div {
    highlight_in(group, radius, inset, selected, colors(cx).background_secondary, cx)
}

pub fn highlight_in(
    group: impl Into<SharedString>,
    radius: f32,
    inset: Edges<f32>,
    selected: bool,
    color: Hsla,
    cx: &App,
) -> Div {
    let c = colors(cx);
    div()
        .absolute()
        .top(px(inset.top))
        .left(px(inset.left))
        .right(px(inset.right))
        .bottom(px(inset.bottom))
        .rounded(px(radius))
        .when(selected, |light| light.bg(c.background_tertiary))
        .when(!selected, |light| light.group_hover(group, move |light| light.bg(color)))
}

pub fn edges(top: f32, left: f32, bottom: f32, right: f32) -> Edges<f32> {
    Edges { top, left, bottom, right }
}

pub fn even(all: f32) -> Edges<f32> {
    edges(all, all, all, all)
}

pub fn tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| component::tooltip::Tooltip::new(text.clone()).build(window, cx)
}

/// A button in the window's top bar. The buttons touch: the space seen between them is theirs.
pub const TOOLBAR_MARGIN: f32 = 2.;
pub const TOOLBAR_WIDTH: f32 = 28. + 2. * TOOLBAR_MARGIN;

/// A thin line between parts of a view.
pub fn divider(cx: &App) -> Div {
    div().h(px(1.)).w_full().flex_shrink_0().bg(colors(cx).border)
}

/// The spinner of a small control.
pub fn spinner(size: f32, cx: &App) -> impl IntoElement {
    Spinner::new(size).render(cx)
}

/// A shadow under an element: its colour, how far down it falls and how soft it is.
pub fn shadow(color: Hsla, y: f32, blur: f32) -> Vec<BoxShadow> {
    vec![BoxShadow { color, offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(0.), inset: false }]
}

/// The window's top bar, which moves the window when it is dragged and zooms it when it is
/// double-clicked, as the Mac's does. What lies on it takes its own clicks.
pub fn window_drag_strip(id: impl Into<ElementId>, height: f32, armed: &Rc<std::cell::Cell<bool>>) -> Stateful<Div> {
    let armed = armed.clone();
    let pressed = armed.clone();
    div()
        .id(id)
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(height))
        .on_mouse_down(MouseButton::Left, move |event, window, _| {
            if event.click_count == 2 {
                pressed.set(false);
                window.titlebar_double_click();
            } else {
                pressed.set(true);
            }
        })
        .on_mouse_up(MouseButton::Left, {
            let armed = armed.clone();
            move |_, _, _| armed.set(false)
        })
        .on_mouse_move(move |event, window, _| {
            if armed.get() && event.pressed_button == Some(MouseButton::Left) {
                armed.set(false);
                window.start_window_move();
            }
        })
}
