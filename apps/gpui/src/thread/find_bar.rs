//! Finding in the open thread, as a browser finds in a page: a card in the thread's top corner,
//! over the toast, with the field, how many matches there are and which one is current, and the
//! buttons that go to the one before and the next and close it. Its last button's corner runs
//! along the card's. `FindBar.swift`.

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::store::Store;
use crate::theme::{ControlSize, Radius, Surface, colors};
use crate::ui::{ActionButton, InputField, InputVariant};

const RADIUS: f32 = Radius::LG;
/// The buttons' corners run along the card's.
const MARGIN: f32 = RADIUS - ControlSize::Regular.radius();
const WIDTH: f32 = 380.;
const HEIGHT: f32 = ControlSize::Regular.height() + 2. * MARGIN;

/// The room above the bar, and between it and the toast under it.
pub const GAP: f32 = 6.;

/// What the bar covers of the transcript's top, which a match is scrolled clear of: the gap above
/// it, itself, and room under it.
pub const COVER: f32 = GAP + HEIGHT + 8.;

pub struct FindBar {
    store: Entity<Store>,
    input: Entity<InputState>,
    /// The store's last ask for the field to take the keys that was done.
    focused_for: u64,
    _subscription: Subscription,
}

impl FindBar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in thread"));
        let subscription = cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| match event {
            InputEvent::Change => {
                let query = input.read(cx).value().to_string();
                this.store.update(cx, |store, cx| {
                    store.set_find_query(query, cx);
                    cx.notify();
                });
            }
            InputEvent::PressEnter { shift, .. } => {
                let back = *shift;
                this.store.update(cx, |store, cx| {
                    store.step_find(back, cx);
                    cx.notify();
                });
            }
            _ => {}
        });
        Self { store, input, focused_for: 0, _subscription: subscription }
    }

    fn close(&self, cx: &mut App) {
        self.store.update(cx, |store, cx| {
            store.close_find(cx);
            cx.notify();
        });
    }
}

impl Render for FindBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = colors(cx);
        let store = self.store.read(cx);
        let find = &store.find;
        if find.focus_requests != self.focused_for {
            self.focused_for = find.focus_requests;
            let query = find.query.clone();
            self.input.update(cx, |input, cx| {
                if input.value() != query.as_str() {
                    input.set_value(query, window, cx);
                }
            });
            // A tick later, once the bar is in the window and whatever had the keys let them go,
            // so that what is typed replaces what the field has.
            cx.on_next_frame(window, |this, window, cx| {
                this.input.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.select_all(window, cx);
                });
            });
        }
        let store = self.store.read(cx);
        let (label, none) = (store.find.label(), store.find.count == 0);
        let previous = store.shortcuts.label_in("find.previous", &["findOpen"]);
        let next = store.shortcuts.label_in("find.next", &["findOpen"]);
        let help = |text: &str, label: Option<String>| match label {
            Some(label) => format!("{text} ({label})"),
            None => text.to_string(),
        };
        let size = ControlSize::Regular;
        let step = |id: &'static str, symbol: &'static str, title: String, back: bool| {
            let store = self.store.clone();
            ActionButton::icon(id, symbol, title).surface(Surface::Popover).disabled(none).on_click(move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.step_find(back, cx);
                    cx.notify();
                })
            })
        };
        let entity = cx.entity().downgrade();
        div()
            .id("find-bar")
            .w(px(WIDTH))
            .h(px(HEIGHT))
            .p(px(MARGIN - 1.))
            .bg(c.popover)
            .rounded(px(RADIUS))
            .border_1()
            .border_color(c.border_popover)
            .block_mouse_except_scroll()
            .flex()
            .items_center()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                    cx.stop_propagation();
                    this.close(cx);
                }
            }))
            .child(div().flex_1().min_w_0().h(px(size.height())).child(
                InputField::new(&self.input).icon("search").variant(InputVariant::Bare).surface(Surface::Popover),
            ))
            .when(!label.is_empty(), |bar| {
                bar.child(
                    div()
                        .flex_none()
                        .px(px(size.gap()))
                        .text_size(px(size.text_size()))
                        .text_color(if none { c.muted_stronger_foreground } else { c.muted_foreground })
                        .child(label),
                )
            })
            .child(
                div().flex_none().mx(px(size.gap())).w(px(1.)).h(px(size.height() - 2. * MARGIN)).bg(c.border_popover),
            )
            .child(step("find-previous", "chevron-up", help("Previous match", previous), true))
            .child(step("find-next", "chevron-down", help("Next match", next), false))
            .child(ActionButton::icon("find-close", "x", "Close").surface(Surface::Popover).on_click(
                move |_, _, cx| {
                    let _ = entity.update(cx, |this, cx| this.close(cx));
                },
            ))
    }
}
