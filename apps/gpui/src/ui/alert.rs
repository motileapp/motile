//! An alert, as macOS 26 draws one: a bold title, what happened, perhaps a field, and capsule
//! buttons side by side along the bottom, the one that cancels first.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{colors, is_dark};
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

    /// The button that goes ahead, in the accent colour.
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
    let dark = is_dark(cx);
    let stacked = buttons.len() > 2;
    let id = id.into();
    let cancel = buttons.iter().find(|button| button.role == Role::Cancel).map(|button| button.on_click.clone());
    let default = buttons.iter().rev().find(|button| button.role != Role::Cancel).map(|button| button.on_click.clone());
    let content = div()
        .pt(px(22.))
        .px(px(22.))
        .pb(px(16.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(div().text_size(px(13.)).line_height(px(16.)).font_weight(FontWeight::BOLD).child(title.into()))
        .when_some(message, |content, message| {
            content.child(div().text_size(px(12.)).line_height(px(16.)).text_color(c.text).child(message))
        })
        .when_some(body, |content, body| content.child(div().w_full().child(body)))
        // Two buttons sit side by side; more stack, the one that cancels last.
        .child(div().mt(px(4.)).mx(px(-6.)).flex().when(stacked, |row| row.flex_col()).gap(px(8.)).children(
            buttons.into_iter().enumerate().map(|(index, button)| {
                let on_click = button.on_click;
                let (background, foreground) = match button.role {
                    Role::Cancel => (if dark { hsla(0., 0., 1., 0.12) } else { hsla(0., 0., 0., 0.08) }, c.text),
                    Role::Default => (c.accent, white()),
                    Role::Destructive => (c.danger.opacity(0.22), c.danger),
                };
                div()
                    .id(("alert-button", index))
                    .when(!stacked, |button| button.flex_1())
                    .h(px(28.))
                    .rounded_full()
                    .bg(background)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(foreground)
                    .active(|button| button.opacity(0.8))
                    .child(button.title)
                    .on_click(move |_, window, cx| on_click(window, cx))
            }),
        ));
    let keys = Keys { id: id.clone(), cancel, default, content: content.into_any_element() };
    sheet_with(id, 260., 24., keys)
}

/// Escape cancels and Return goes ahead, as in the Mac's alerts. The alert takes the keys from
/// the start, unless its field already has them.
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
