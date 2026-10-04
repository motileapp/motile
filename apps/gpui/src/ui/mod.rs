//! The small pieces every screen is made of: the light under the pointer, buttons that are only
//! a symbol or only their text, and the icons of agents and projects.

pub mod alert;
pub mod button;
pub mod icons;
pub mod logos;
pub mod menu;
pub mod rich_text;
pub mod sheet;

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::colors;

pub type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The light under what the pointer is over, and under what is selected. It is drawn `inset`
/// from the edges of the element `group` names: that margin looks empty but is the element's,
/// so neighbours leave no gap to miss.
pub fn highlight(group: impl Into<SharedString>, radius: f32, inset: Edges<f32>, selected: bool, cx: &App) -> Div {
    highlight_in(group, radius, inset, selected, colors(cx).hover, cx)
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
        .when(selected, |light| light.bg(c.selected))
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

/// A button that is only a symbol, with room around it to hit and a background under the pointer.
/// The inset is more room to hit, outside the background.
#[derive(IntoElement)]
pub struct IconButton {
    id: ElementId,
    symbol: &'static str,
    help: Option<SharedString>,
    size: f32,
    symbol_size: f32,
    radius: f32,
    inset: Edges<f32>,
    color: Option<Hsla>,
    on_click: Option<OnClick>,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, symbol: &'static str) -> Self {
        Self {
            id: id.into(),
            symbol,
            help: None,
            size: 26.,
            symbol_size: 13.,
            radius: 6.,
            inset: even(0.),
            color: None,
            on_click: None,
        }
    }

    pub fn help(mut self, help: impl Into<SharedString>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn symbol_size(mut self, size: f32) -> Self {
        self.symbol_size = size;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn inset(mut self, inset: Edges<f32>) -> Self {
        self.inset = inset;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for IconButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let group: SharedString = format!("icon-button-{:?}", self.id).into();
        let color = self.color.unwrap_or(c.text);
        div()
            .id(self.id)
            .group(group.clone())
            .relative()
            .flex_shrink_0()
            .occlude()
            .pt(px(self.inset.top))
            .pl(px(self.inset.left))
            .pb(px(self.inset.bottom))
            .pr(px(self.inset.right))
            .child(highlight(group, self.radius, self.inset, false, cx))
            .child(
                div()
                    .size(px(self.size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(color)
                    .child(icons::symbol(self.symbol, self.symbol_size)),
            )
            .when_some(self.help, |button, help| button.tooltip(tooltip(help)))
            .when_some(self.on_click, |button, on_click| {
                button.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_click(event, window, cx)
                })
            })
    }
}

/// A button in the window's top bar. It lights up as a rounded rectangle, like a row of the
/// sidebar, and the buttons touch: the space seen between them is theirs.
pub const TOOLBAR_MARGIN: f32 = 2.;
pub const TOOLBAR_WIDTH: f32 = 28. + 2. * TOOLBAR_MARGIN;

pub fn toolbar_button(id: impl Into<ElementId>, symbol: &'static str, help: impl Into<SharedString>) -> IconButton {
    IconButton::new(id, symbol)
        .help(help)
        .size(TOOLBAR_WIDTH - 2. * TOOLBAR_MARGIN)
        .symbol_size(15.)
        .radius(7.)
        .inset(even(TOOLBAR_MARGIN))
}

/// A button that is only its text, in the colour of a link. The background under the pointer
/// and the room to hit reach past the text, into the space around.
pub fn link_button(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let c = colors(cx);
    let id = id.into();
    let group: SharedString = format!("link-{id:?}").into();
    div()
        .id(id)
        .group(group.clone())
        .relative()
        .h(px(30.))
        .min_w(px(30.))
        .px(px(8.))
        .mx(px(-8.))
        .flex()
        .items_center()
        .justify_center()
        .child(highlight_in(group, 6., even(0.), false, c.link_hover, cx))
        .child(div().relative().text_size(px(12.)).text_color(c.link).child(title.into()))
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx)
        })
}

/// A small button that says what it does.
pub fn pill_button(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    cx: &App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let c = colors(cx);
    div()
        .id(id.into())
        .flex_shrink_0()
        .h(px(20.))
        .px(px(9.))
        .flex()
        .items_center()
        .rounded_full()
        .bg(c.hover)
        .hover(|pill| pill.bg(c.selected))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c.text)
        .child(title.into())
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx)
        })
}

/// A thin line between parts of a view.
pub fn divider(cx: &App) -> Div {
    div().h(px(1.)).w_full().flex_shrink_0().bg(colors(cx).border)
}

/// The spinner of a small control, as the Mac draws it: spokes going round.
pub fn spinner(size: f32, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    svg().path("icons/loader.svg").size(px(size)).flex_shrink_0().text_color(c.secondary).with_animation(
        "spinner",
        Animation::new(std::time::Duration::from_millis(1000)).repeat(),
        |icon, delta| icon.with_transformation(Transformation::rotate(percentage((delta * 8.).floor() / 8.))),
    )
}

/// A shadow under an element: its colour, how far down it falls and how soft it is.
pub fn shadow(color: Hsla, y: f32, blur: f32) -> Vec<BoxShadow> {
    vec![BoxShadow { color, offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(0.), inset: false }]
}

/// A text field as the Mac draws one: white, lightly bordered, with the focus ring around it.
pub fn text_field(state: &Entity<component::input::InputState>, _cx: &App) -> TextField {
    TextField { state: state.clone() }
}

#[derive(IntoElement)]
pub struct TextField {
    state: Entity<component::input::InputState>,
}

impl RenderOnce for TextField {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let focused = self.state.read(cx).focus_handle(cx).is_focused(window);
        div()
            .h(px(22.))
            .px(px(4.))
            .rounded(px(5.))
            .bg(c.raised)
            .border_1()
            .border_color(if focused { c.primary.opacity(0.6) } else { c.strong_border })
            .when(focused, |field| {
                field.shadow(vec![BoxShadow {
                    color: c.primary.opacity(0.35),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.),
                    inset: false,
                }])
            })
            .flex()
            .items_center()
            .child(
                component::input::Input::new(&self.state).appearance(false).px_0().py_0().h(px(20.)).text_size(px(13.)),
            )
    }
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
