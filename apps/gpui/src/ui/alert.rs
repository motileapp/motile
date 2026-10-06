//! An alert: a card with a title, what happened, perhaps a field, and the buttons along its
//! bottom, the one that cancels first and the one that goes ahead last.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{Radius, Surface, colors};
use crate::ui::control::{ActionButton, Variant};
use crate::ui::sheet::{Sheet, sheet_with};

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Cancel,
    Default,
    Destructive,
}

type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// One of the buttons along the bottom of an alert.
pub struct AlertButton {
    title: SharedString,
    role: Role,
    on_click: Run,
}

impl AlertButton {
    pub fn new(title: impl Into<SharedString>, on_click: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self { title: title.into(), role: Role::Cancel, on_click: Rc::new(on_click) }
    }

    /// The button that goes ahead, in the primary colour.
    pub fn prominent(mut self) -> Self {
        self.role = Role::Default;
        self
    }

    pub fn destructive(mut self) -> Self {
        self.role = Role::Destructive;
        self
    }
}

pub fn alert(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    message: Option<SharedString>,
    body: Option<AnyElement>,
    buttons: Vec<AlertButton>,
    cx: &App,
) -> Sheet {
    let c = colors(cx);
    let id = id.into();
    let cancel = buttons.iter().find(|button| button.role == Role::Cancel).map(|button| button.on_click.clone());
    let default = buttons.iter().rev().find(|button| button.role != Role::Cancel).map(|button| button.on_click.clone());
    let content = div()
        .p(px(20.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(div().text_size(px(15.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).child(title.into()))
        .when_some(message, |content, message| {
            content.child(div().text_size(px(13.)).line_height(px(18.)).text_color(c.secondary).child(message))
        })
        .when_some(body, |content, body| content.child(div().w_full().pt(px(4.)).child(body)))
        .child(div().mt(px(12.)).flex().justify_end().gap(px(8.)).children(buttons.into_iter().enumerate().map(
            |(index, button)| {
                let on_click = button.on_click;
                let variant = match button.role {
                    Role::Cancel => Variant::Secondary,
                    Role::Default => Variant::Primary,
                    Role::Destructive => Variant::Danger,
                };
                ActionButton::new(("alert-button", index), button.title)
                    .variant(variant)
                    .surface(Surface::Popover)
                    .on_click(move |_, window, cx| on_click(window, cx))
            },
        )));
    let keys = Keys { id: id.clone(), cancel, default, content: content.into_any_element() };
    sheet_with(id, 400., Radius::SHEET, keys)
}

/// Escape cancels and Return goes ahead. The alert takes the keys from the start, unless its
/// field already has them.
#[derive(IntoElement)]
struct Keys {
    id: ElementId,
    cancel: Option<Run>,
    default: Option<Run>,
    content: AnyElement,
}

impl RenderOnce for Keys {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window
            .use_keyed_state(self.id.clone(), cx, |_, cx| (cx.focus_handle(), Rc::new(std::cell::Cell::new(false))));
        let (focus, laid_out) = state.read(cx).clone();
        // On its first frame the alert isn't in the tree yet to tell whether its field has the keys.
        if !laid_out.replace(true) {
            window.request_animation_frame();
        } else if !focus.contains_focused(window, cx) {
            focus.focus(window, cx);
        }
        let (cancel, default) = (self.cancel, self.default);
        div()
            .track_focus(&focus)
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                let run = match event.keystroke.key.as_str() {
                    "escape" => cancel.clone(),
                    "enter" => default.clone(),
                    _ => None,
                };
                if let Some(run) = run {
                    cx.stop_propagation();
                    run(window, cx);
                }
            })
            .child(self.content)
    }
}
