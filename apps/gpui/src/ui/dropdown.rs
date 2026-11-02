//! What drops from a button the app draws, as the Mac's `dropdown` modifier has it: under the
//! button, or over it where there is too little room under it, lined up with its leading or its
//! trailing edge. `ChoiceDropdown` is a button with a short list to pick one from.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{IconSize, Radius, Surface, colors};
use crate::ui::menu::Anchor;
use crate::ui::{ActionButton, Highlight, Variant, dropdown_card, edges, icons};
use crate::window_frame::content_bounds;

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// The room between a dropdown and its button.
const GAP: f32 = 4.;
/// A dropdown with less room than this under its button, and more over it, opens over it.
const ROOM_BELOW: f32 = 220.;

/// Where a dropdown stands over the window's content, which the caller fills with it: against the
/// button whose light is `button`, under it or over it, and lined up with its `trailing` edge or
/// its leading one.
pub fn drop_from(button: Bounds<Pixels>, trailing: bool, window: &Window) -> Div {
    let content = content_bounds(window);
    let below = content.bottom() - button.bottom();
    let above = button.top() - content.top();
    let up = below < px(ROOM_BELOW) && above > below;
    let place = div().absolute();
    let place = if trailing {
        place.right(content.right() - button.right())
    } else {
        place.left(button.left() - content.left())
    };
    if up {
        return place.bottom(content.bottom() - button.top() + px(GAP));
    }
    place.top(button.bottom() - content.top() + px(GAP))
}

/// A layer as big as the window's content, over everything but the menus.
pub fn over_content(layer: impl IntoElement, window: &Window) -> Deferred {
    deferred(anchored().position(crate::window_frame::content_origin(window)).child(layer)).with_priority(1)
}

/// The light of a button, `margin` inside the bounds that take its presses.
pub fn light(bounds: Bounds<Pixels>, margin: Edges<f32>) -> Bounds<Pixels> {
    Bounds::from_corners(
        point(bounds.left() + px(margin.left), bounds.top() + px(margin.top)),
        point(bounds.right() - px(margin.right), bounds.bottom() - px(margin.bottom)),
    )
}

/// One of what a `ChoiceDropdown` offers: its symbol or picture, its name, a line about it, and
/// whether it is the one chosen.
pub struct DropdownChoice {
    title: SharedString,
    detail: Option<SharedString>,
    icon: Option<AnyElement>,
    chosen: bool,
    choose: Run,
}

impl DropdownChoice {
    pub fn new(title: impl Into<SharedString>, chosen: bool, choose: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self { title: title.into(), detail: None, icon: None, chosen, choose: Rc::new(choose) }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn symbol(mut self, symbol: &'static str) -> Self {
        self.icon = Some(icons::symbol(symbol, IconSize::Md).into_any_element());
        self
    }

    pub fn picture(mut self, picture: impl IntoElement) -> Self {
        self.icon = Some(picture.into_any_element());
        self
    }
}

/// A ghost button that drops a short list to pick one from, under a title, as the composer's
/// strip offers its server and its workspace. Picking one, Esc or a press elsewhere closes it.
#[derive(IntoElement)]
pub struct ChoiceDropdown {
    id: ElementId,
    button: ActionButton,
    title: SharedString,
    choices: Vec<DropdownChoice>,
    trailing: bool,
    shortcut: Option<&'static str>,
    on_close: Option<Run>,
}

/// Whether a `ChoiceDropdown` is open, where its button was drawn, and what has the keys while
/// it is.
struct Dropped {
    open: bool,
    anchor: Anchor,
    focus: FocusHandle,
}

impl ChoiceDropdown {
    const PADDING: f32 = 4.;
    const ROW_HEIGHT: f32 = 28.;

    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        title: impl Into<SharedString>,
        choices: Vec<DropdownChoice>,
    ) -> Self {
        let id = id.into();
        let button = ActionButton::new(id.clone(), label).variant(Variant::Ghost).opens(true);
        Self { id, button, title: title.into(), choices, trailing: false, shortcut: None, on_close: None }
    }

    /// Changes the button it drops from.
    pub fn button(mut self, change: impl FnOnce(ActionButton) -> ActionButton) -> Self {
        self.button = change(self.button);
        self
    }

    /// It lines up with its button's trailing edge.
    pub fn trailing(mut self) -> Self {
        self.trailing = true;
        self
    }

    /// Makes the button the one the command's keys press.
    pub fn shortcut(mut self, command: &'static str) -> Self {
        self.shortcut = Some(command);
        self
    }

    /// What runs once it closes, as giving the keys back to what had them.
    pub fn on_close(mut self, on_close: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(on_close));
        self
    }

    /// How wide its button is when nothing cuts its words short.
    pub fn natural_width(&self, window: &mut Window, cx: &App) -> f32 {
        self.button.natural_width(window, cx)
    }

    fn row(index: usize, choice: DropdownChoice, close: Run, cx: &App) -> Stateful<Div> {
        let c = colors(cx);
        let group = SharedString::from(format!("choice-{index}"));
        let choose = choice.choose;
        div()
            .id(("choice", index))
            .group(group.clone())
            .relative()
            .min_h(px(Self::ROW_HEIGHT))
            .px(px(8.))
            .when(choice.detail.is_some(), |row| row.py(px(6.)))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                Highlight::new()
                    .radius(Radius::SM)
                    .inset(edges(1., 0., 1., 0.))
                    .surface(Surface::Popover)
                    .render(group, cx),
            )
            .children(
                choice
                    .icon
                    .map(|icon| div().relative().flex_shrink_0().flex().text_color(c.muted_foreground).child(icon)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .whitespace_nowrap()
                    .child(div().text_size(px(13.)).text_color(c.foreground).child(choice.title))
                    .children(
                        choice
                            .detail
                            .map(|detail| div().text_size(px(12.)).text_color(c.muted_foreground).child(detail)),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .ml(px(4.))
                    .text_color(c.foreground)
                    .when(!choice.chosen, |mark| mark.opacity(0.))
                    .child(icons::symbol("check", IconSize::Sm)),
            )
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                choose(window, cx);
                close(window, cx);
            })
    }
}

impl RenderOnce for ChoiceDropdown {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let key = ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "dropdown".into());
        let state = window.use_keyed_state(key, cx, |_, cx| Dropped {
            open: false,
            anchor: Anchor::default(),
            focus: cx.focus_handle(),
        });
        let (open, anchor, focus) = {
            let dropped = state.read(cx);
            (dropped.open, dropped.anchor.clone(), dropped.focus.clone())
        };
        let on_close = self.on_close.clone();
        let show = Rc::new({
            let state = state.clone();
            move |shown: bool, window: &mut Window, cx: &mut App| {
                let focus = state.update(cx, |dropped, _| {
                    dropped.open = shown;
                    dropped.focus.clone()
                });
                if shown {
                    crate::ui::overlay_opened(cx);
                    window.focus(&focus, cx);
                } else if let Some(on_close) = &on_close {
                    on_close(window, cx);
                }
                window.refresh();
            }
        });
        let margin = self.button.margin_edges();
        let button = self.button.open(open).on_click({
            let show = show.clone();
            move |_, window, cx| show(true, window, cx)
        });
        let pressed = {
            let show = show.clone();
            move |window: &mut Window, cx: &mut App| show(!open, window, cx)
        };
        let dropdown = open.then(|| {
            let close: Run = {
                let show = show.clone();
                Rc::new(move |window, cx| show(false, window, cx))
            };
            let c = colors(cx);
            let rows =
                self.choices.into_iter().enumerate().map(|(index, choice)| Self::row(index, choice, close.clone(), cx));
            let card = dropdown_card(div(), cx)
                .id("choice-dropdown")
                .min_w(px(200.))
                .p(px(Self::PADDING))
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .px(px(8.))
                        .pt(px(6.))
                        .pb(px(4.))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(c.muted_foreground)
                        .child(self.title),
                )
                .children(rows);
            let content = content_bounds(window).size;
            let escape = close.clone();
            let layer = div()
                .id("choice-dropdown-layer")
                .w(content.width)
                .h(content.height)
                .occlude()
                .track_focus(&focus)
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key != "escape" {
                        return;
                    }
                    cx.stop_propagation();
                    escape(window, cx)
                })
                .on_mouse_down(MouseButton::Left, move |_, window, cx| close(window, cx))
                .child(drop_from(light(anchor.bounds(), margin), self.trailing, window).child(card));
            over_content(layer, window)
        });
        div()
            .relative()
            .flex_shrink_0()
            .child(anchor.track())
            .children(self.shortcut.map(|command| crate::keyboard::target_pressing(command, pressed)))
            .child(button)
            .children(dropdown)
    }
}
