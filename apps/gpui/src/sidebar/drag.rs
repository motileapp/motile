//! Moving an active thread to another place in the list with `Reorder`: held or pulled, a copy of
//! its row floats under the pointer over everything in the window, and let go where it was held,
//! it is clicked. And pulling a shelf's line.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::rows::row_light;
use super::{Sidebar, row_margin};
use crate::models::ThreadInfo;
use crate::store::Selection;
use crate::theme::{Layer, Radius, Surface, colors};
use crate::ui::reorder::{Handlers, Reorder};

/// The thread a row moves, and whether the pointer was on the row as it was pressed, which its
/// floating copy keeps the look of.
pub(super) type Moving = (ThreadInfo, bool);

pub(super) const THREAD_ORDER: Handlers<Sidebar, Moving> = Handlers {
    reorder: |this| &mut this.reorder,
    moved: |this, (thread, _), index, cx| {
        let rows = this.plan.as_ref().map(|plan| plan.row_threads.clone()).unwrap_or_default();
        this.store.update(cx, |store, cx| {
            store.move_thread(&thread, index, &rows);
            cx.notify();
        });
    },
    clicked: Some(|this, (thread, _), cx| {
        this.store.update(cx, |store, cx| {
            store.select(Selection::Thread(thread.id.clone()), cx);
            cx.notify();
        });
    }),
};

impl Sidebar {
    /// A row that moves was pressed: it lifts once it is held, or pulled, among the rows that move.
    pub(super) fn press(&mut self, index: usize, thread: ThreadInfo, start: Point<Pixels>, cx: &mut Context<Self>) {
        self.peek.hide();
        let Some(plan) = self.plan.clone() else { return };
        let layout = &plan.layout;
        let width = self.list.bounds().size.width;
        let bounds = layout
            .tops
            .iter()
            .zip(&layout.heights)
            .map(|(top, height)| Bounds { origin: point(px(0.), px(*top)), size: size(width, px(*height)) })
            .collect();
        self.reorder.measure_content(bounds);
        let first = plan.movable.iter().position(|movable| *movable).unwrap_or(index);
        let last = plan.movable.iter().rposition(|movable| *movable).unwrap_or(index);
        let hovered = self.hovering(&format!("thread-{}", thread.id));
        self.reorder.press((thread, hovered), index, first..last + 1, start, THREAD_ORDER, cx);
    }

    /// Where the list's content is in the window, which the lifted row is measured from.
    pub(super) fn keep_frame(&self) {
        let bounds = self.list.bounds();
        self.reorder.set_frame(point(bounds.origin.x, bounds.origin.y - px(self.list.scroll_top())));
    }

    /// The row that is lifted, floating where the pointer has it, over everything in the window,
    /// on its light, as it looked when it was pressed.
    pub(super) fn lift_layer(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (thread, hovered) = self.reorder.lifted_key()?.clone();
        let floating = self.reorder.floating(window)?;
        let selected = self.store.read(cx).selection == Selection::Thread(thread.id.clone());
        let key = format!("lifted-{}", thread.id);
        let margin = row_margin();
        let c = colors(cx);
        let content = self.thread_row_content(&thread, selected, hovered, None, cx);
        // It lies on the list's own colour and takes the pointer, so the rows it passes over
        // neither show through nor light up.
        let row = div()
            .id("lifted-row")
            .occlude()
            .w(floating.size.width)
            .h(floating.size.height)
            .relative()
            .pt(px(margin.top))
            .pb(px(margin.bottom))
            .px(px(margin.left))
            .child(
                div()
                    .absolute()
                    .top(px(margin.top))
                    .bottom(px(margin.bottom))
                    .left(px(margin.left))
                    .right(px(margin.right))
                    .rounded(px(Radius::MD))
                    .bg(Surface::Background.color(c))
                    .child(
                        div()
                            .size_full()
                            .rounded(px(Radius::MD))
                            .bg(Surface::Background.layer(Layer::Row, c))
                            .opacity(1. - floating.settled),
                    ),
            )
            .child(row_light(&key, selected, hovered, cx))
            .child(content);
        Some(Reorder::<Moving>::float(floating.origin, row))
    }

    /// While a shelf's line is pulled, the window's every move and release goes to the sidebar,
    /// wherever the pointer is.
    pub(super) fn follow_pulling(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.pulling?;
        let this = cx.entity().downgrade();
        Some(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let moved = this.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let _ = moved.update(cx, |this, cx| {
                            let Some((shelf, start, _)) = this.pulling else { return };
                            if event.pressed_button != Some(MouseButton::Left) {
                                return this.finish_pulling(cx);
                            }
                            this.pulling = Some((shelf, start, f32::from(event.position.y)));
                            cx.notify();
                        });
                    });
                    let released = this.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                            return;
                        }
                        let _ = released.update(cx, |this, cx| {
                            if this.pulling.is_some() {
                                this.finish_pulling(cx);
                            }
                        });
                    });
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        )
    }
}
