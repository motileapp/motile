//! Moving an active thread to another place in the list: held or pulled, a picture of its row
//! floats under the pointer over everything in the window, the rows it passes slide out of its
//! way, and let go it slides into its place. Let go where it was held, it is clicked:
//! `RecycledListMac.swift`'s `RecycledTable` and `RowLift`.

use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::rows::row_light;
use super::{Sidebar, row_margin};
use crate::models::ThreadInfo;
use crate::store::Selection;
use crate::theme::{Layer, Radius, Surface, colors};
use crate::ui::ease_out;

/// How long a press is held before the row lifts, and how far it is pulled.
const HOLD: Duration = Duration::from_millis(200);
const PULL: f32 = 3.;
const SLIDE: Duration = Duration::from_millis(200);
const DROP: Duration = Duration::from_millis(250);
/// Above the rows and the card, under the menus and the modals.
const PRIORITY: usize = 20;

/// A press on a row that moves, which lifts it once it is held or pulled.
pub struct Press {
    index: usize,
    thread: ThreadInfo,
    start: Point<Pixels>,
    pulled: bool,
    _hold: Task<()>,
}

/// A row on its way: where it was picked up, where it floats, and where it would land.
pub struct Lift {
    from: usize,
    index: usize,
    thread: ThreadInfo,
    /// The pointer was on the row as it lifted, which it keeps the look of.
    hovered: bool,
    /// How far below the row's top it was picked up.
    grip: f32,
    tops: Vec<f32>,
    heights: Vec<f32>,
    /// Where it can land, in the list without it.
    landings: (usize, usize),
    /// Where the pointer is from the row's origin.
    offset: Point<Pixels>,
    pointer: Point<Pixels>,
    width: Pixels,
    /// How far each row slides out of the way: from, to, and when it began.
    shifts: Vec<(f32, f32, Instant)>,
    dropping: Option<(Point<Pixels>, Point<Pixels>, Instant)>,
}

fn eased(started: Instant, duration: Duration) -> f32 {
    ease_out((started.elapsed().as_secs_f32() / duration.as_secs_f32()).min(1.))
}

impl Lift {
    /// How far the row is slid out of the way, with the gap where it would land.
    fn shift(&self, row: usize) -> f32 {
        let height = self.heights[self.from];
        if row > self.from && row <= self.index {
            -height
        } else if row < self.from && row >= self.index {
            height
        } else {
            0.
        }
    }

    /// Where the row is drawn now, on its way to where it slides.
    pub fn shown_shift(&self, row: usize) -> f32 {
        let Some((from, to, started)) = self.shifts.get(row) else { return 0. };
        from + (to - from) * eased(*started, SLIDE)
    }

    fn middle(&self, row: usize) -> f32 {
        self.tops[row] + self.heights[row] / 2. + self.shift(row)
    }

    /// Follows the pointer, at `y` in the list's content, and parts the rows where it would land.
    fn follow(&mut self, y: f32) {
        let center = y - self.grip + self.heights[self.from] / 2.;
        let landing = self.index;
        while self.index < self.landings.1 {
            let below = if self.index < self.from { self.index } else { self.index + 1 };
            if below >= self.tops.len() || center < self.middle(below) {
                break;
            }
            self.index += 1;
        }
        while self.index > self.landings.0 {
            let above = if self.index <= self.from { self.index - 1 } else { self.index };
            if center > self.middle(above) {
                break;
            }
            self.index -= 1;
        }
        if landing == self.index {
            return;
        }
        let now = Instant::now();
        for row in 0..self.tops.len() {
            let target = self.shift(row);
            if self.shifts[row].1 != target {
                self.shifts[row] = (self.shown_shift(row), target, now);
            }
        }
    }

    /// Whether anything still slides.
    fn moving(&self) -> bool {
        self.dropping.is_some() || self.shifts.iter().any(|(_, _, started)| started.elapsed() < SLIDE)
    }
}

impl Sidebar {
    /// A row that moves was pressed: it lifts once it is held, or pulled.
    pub(super) fn press(
        &mut self,
        index: usize,
        thread: ThreadInfo,
        start: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.peek.hide();
        if self.lift.is_some() {
            return;
        }
        let hold = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOLD).await;
            let _ = this.update(cx, |this, cx| this.pick_up(cx));
        });
        self.press = Some(Press { index, thread, start, pulled: false, _hold: hold });
        cx.notify();
    }

    fn pick_up(&mut self, cx: &mut Context<Self>) {
        let Some(press) = self.press.as_ref() else { return };
        if self.lift.is_some() {
            return;
        }
        let Some(plan) = self.plan.clone() else { return };
        let layout = &plan.layout;
        let from = press.index;
        if from >= layout.tops.len() {
            return;
        }
        let others: Vec<usize> = (0..layout.tops.len())
            .filter(|row| *row != from && plan.movable.get(*row).copied().unwrap_or(false))
            .map(|row| if row > from { row - 1 } else { row })
            .collect();
        let landings = match (others.first(), others.last()) {
            (Some(first), Some(last)) => (*first, last + 1),
            _ => (from, from),
        };
        let bounds = self.list.bounds();
        let origin = point(bounds.origin.x, px(self.list.row_top(layout, from)));
        let grip = self.list.content_y(f32::from(press.start.y)) - layout.tops[from];
        let now = Instant::now();
        let hovered = self.hovering(&format!("thread-{}", press.thread.id));
        self.lift = Some(Lift {
            from,
            index: from,
            thread: press.thread.clone(),
            hovered,
            grip,
            tops: layout.tops.clone(),
            heights: layout.heights.clone(),
            landings,
            offset: press.start - origin,
            pointer: press.start,
            width: bounds.size.width,
            shifts: vec![(0., 0., now); layout.tops.len()],
            dropping: None,
        });
        cx.notify();
    }

    /// The pointer moved while a row is pressed or lifted.
    pub(super) fn drag_moved(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let pulls = match self.press.as_mut() {
            Some(press) if !press.pulled => {
                let (dx, dy) = (f32::from(position.x - press.start.x), f32::from(position.y - press.start.y));
                if dx.hypot(dy) < PULL {
                    return;
                }
                press.pulled = true;
                true
            }
            _ => false,
        };
        if pulls {
            self.pick_up(cx);
        }
        let y = self.list.content_y(f32::from(position.y));
        let Some(lift) = self.lift.as_mut().filter(|lift| lift.dropping.is_none()) else { return };
        lift.pointer = position;
        lift.follow(y);
        cx.notify();
    }

    /// The press ended: a row that never lifted, or was let go where it was held, is clicked; a
    /// lifted one slides into its place, and is moved there once it has.
    pub(super) fn drag_released(&mut self, cx: &mut Context<Self>) {
        let Some(press) = self.press.take() else { return };
        let clicked = !press.pulled;
        if clicked {
            let id = press.thread.id.clone();
            self.store.update(cx, |store, cx| {
                store.select(Selection::Thread(id), cx);
                cx.notify();
            });
        }
        let Some(lift) = self.lift.as_mut() else { return cx.notify() };
        let top = if lift.index <= lift.from {
            lift.tops[lift.index]
        } else {
            lift.tops[lift.index] + lift.heights[lift.index] - lift.heights[lift.from]
        };
        let bounds = self.list.bounds();
        let landing = point(bounds.origin.x, bounds.origin.y + px(top - self.list.scroll_top()));
        lift.dropping = Some((lift.pointer - lift.offset, landing, Instant::now()));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DROP).await;
            let _ = this.update(cx, |this, cx| this.land(cx));
        })
        .detach();
        cx.notify();
    }

    /// Moves the thread where it landed.
    fn land(&mut self, cx: &mut Context<Self>) {
        let Some(lift) = self.lift.take() else { return };
        if lift.index != lift.from {
            let rows = self.plan.as_ref().map(|plan| plan.row_threads.clone()).unwrap_or_default();
            self.store.update(cx, |store, cx| {
                store.move_thread(&lift.thread, lift.index, &rows);
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Puts the rows back where they are laid out, for when the list changes under a lift.
    pub(super) fn cancel_lift(&mut self) {
        self.press = None;
        self.lift = None;
    }

    /// The row that is lifted, floating where the pointer has it, over everything in the window,
    /// on its light, as it looked when it lifted.
    pub(super) fn lift_layer(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let lift = self.lift.as_ref()?;
        if lift.moving() {
            window.request_animation_frame();
        }
        let (origin, light) = match lift.dropping {
            Some((from, to, started)) => {
                let progress = eased(started, DROP);
                (point(from.x + (to.x - from.x) * progress, from.y + (to.y - from.y) * progress), 1. - progress)
            }
            None => (lift.pointer - lift.offset, 1.),
        };
        let height = lift.heights[lift.from];
        let selected = self.store.read(cx).selection == Selection::Thread(lift.thread.id.clone());
        let key = format!("lifted-{}", lift.thread.id);
        let margin = row_margin();
        let c = colors(cx);
        let content = self.thread_row_content(&lift.thread, selected, lift.hovered, None, cx);
        let row = div()
            .w(lift.width)
            .h(px(height))
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
                    .bg(Surface::Background.layer(Layer::Row, c))
                    .opacity(light),
            )
            .child(row_light(&key, selected, lift.hovered, cx))
            .child(content);
        Some(deferred(anchored().position(origin).child(row)).with_priority(PRIORITY).into_any_element())
    }

    /// While a row is pressed or a shelf's line pulled, the window's every move and release goes
    /// to the sidebar, wherever the pointer is.
    pub(super) fn follow_pointer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.press.is_none() && self.pulling.is_none() {
            return None;
        }
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
                            if let Some((shelf, start, _)) = this.pulling {
                                if event.pressed_button != Some(MouseButton::Left) {
                                    return this.finish_pulling(cx);
                                }
                                this.pulling = Some((shelf, start, f32::from(event.position.y)));
                                return cx.notify();
                            }
                            this.drag_moved(event.position, cx);
                        });
                    });
                    let released = this.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                            return;
                        }
                        let _ = released.update(cx, |this, cx| {
                            if this.pulling.is_some() {
                                return this.finish_pulling(cx);
                            }
                            this.drag_released(cx);
                        });
                    });
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        )
    }

    /// Where an active row is drawn: slid out of the way of a lifted one, or hidden where the
    /// lifted one was.
    pub(super) fn active_shift(&self, row: usize) -> Option<f32> {
        let Some(lift) = self.lift.as_ref() else { return Some(0.) };
        if row == lift.from {
            return None;
        }
        Some(lift.shown_shift(row))
    }
}
