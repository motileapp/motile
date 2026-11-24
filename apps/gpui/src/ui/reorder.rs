//! Moving one of a row or a column of things to another place among them, as the sidebar's
//! `drag.rs` moves a thread: pulled, it lifts and follows the pointer along them, the others it
//! passes slide out of its way, and let go it slides into its place. A press that isn't pulled
//! stays a click.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::*;

use super::motion::Tween;

const PULL: f32 = 3.;
const SLIDE: Duration = Duration::from_millis(200);
const DROP: Duration = Duration::from_millis(250);
/// Above its neighbours, under the menus and the modals.
const PRIORITY: usize = 20;

/// The things that can be moved, of keys `K`, laid out along `axis`.
pub struct Reorder<K> {
    axis: Axis,
    /// Where each thing was last laid out along the axis: its start and its length.
    spans: Rc<RefCell<Vec<(f32, f32)>>>,
    press: Option<Press<K>>,
    lift: Option<Lift<K>>,
}

struct Press<K> {
    key: K,
    index: usize,
    count: usize,
    start: Point<Pixels>,
    pulled: bool,
}

struct Lift<K> {
    key: K,
    from: usize,
    /// Where it would land, among the others.
    index: usize,
    spans: Vec<(f32, f32)>,
    /// How far it is pulled from its place.
    pulled: f32,
    shifts: Vec<Tween>,
    dropping: Option<Tween>,
}

impl<K> Lift<K> {
    fn length(&self) -> f32 {
        self.spans[self.from].1
    }

    /// How far a thing is slid out of the way, with the gap where the lifted one would land.
    fn shift(&self, item: usize) -> f32 {
        if item > self.from && item <= self.index {
            -self.length()
        } else if item < self.from && item >= self.index {
            self.length()
        } else {
            0.
        }
    }

    fn middle(&self, item: usize) -> f32 {
        let (start, length) = self.spans[item];
        start + length / 2. + self.shift(item)
    }

    /// Follows the pointer `pulled` from where it was pressed, kept between the first thing's
    /// start and the last one's end, and parts the others where it would land.
    fn follow(&mut self, pulled: f32) {
        let (first, last) = (self.spans[0], self.spans[self.spans.len() - 1]);
        let start = self.spans[self.from].0;
        self.pulled = pulled.clamp(first.0 - start, (last.0 + last.1 - self.length() - start).max(first.0 - start));
        let center = start + self.pulled + self.length() / 2.;
        let landing = self.index;
        while self.index + 1 < self.spans.len() {
            let next = if self.index < self.from { self.index } else { self.index + 1 };
            if center < self.middle(next) {
                break;
            }
            self.index += 1;
        }
        while self.index > 0 {
            let previous = if self.index <= self.from { self.index - 1 } else { self.index };
            if center > self.middle(previous) {
                break;
            }
            self.index -= 1;
        }
        if landing == self.index {
            return;
        }
        for item in 0..self.spans.len() {
            let target = self.shift(item);
            self.shifts[item].retarget(target);
        }
    }

    /// How far its place is from where it was laid out.
    fn landing(&self) -> f32 {
        let (start, length) = self.spans[self.index];
        let from = self.spans[self.from].0;
        if self.index <= self.from { start - from } else { start + length - self.length() - from }
    }

    fn moving(&self) -> bool {
        self.dropping.is_some() || self.shifts.iter().any(Tween::moving)
    }
}

fn spans_along(axis: Axis, bounds: &[Bounds<Pixels>]) -> Vec<(f32, f32)> {
    bounds
        .iter()
        .map(|bounds| match axis {
            Axis::Horizontal => (f32::from(bounds.origin.x), f32::from(bounds.size.width)),
            Axis::Vertical => (f32::from(bounds.origin.y), f32::from(bounds.size.height)),
        })
        .collect()
}

impl<K: Clone + 'static> Reorder<K> {
    pub fn new(axis: Axis) -> Self {
        Self { axis, spans: Rc::default(), press: None, lift: None }
    }

    fn along(&self, point: Point<Pixels>) -> f32 {
        f32::from(match self.axis {
            Axis::Horizontal => point.x,
            Axis::Vertical => point.y,
        })
    }

    /// Keeps where the things are laid out, for the `on_children_prepainted` of what holds them.
    pub fn measure(&self) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let (spans, axis) = (self.spans.clone(), self.axis);
        move |bounds, _, _| *spans.borrow_mut() = spans_along(axis, &bounds)
    }

    /// Keeps where the things are laid out in what scrolls with `handle`, whose children GPUI
    /// gives no `on_children_prepainted`.
    pub fn measure_scrolled(&self, handle: &ScrollHandle) {
        let bounds: Vec<Bounds<Pixels>> =
            (0..handle.children_count()).filter_map(|ix| handle.bounds_for_item(ix)).collect();
        *self.spans.borrow_mut() = spans_along(self.axis, &bounds);
    }

    /// The thing at `index` of `count` was pressed, which lifts once it is pulled.
    pub fn press(&mut self, key: K, index: usize, count: usize, start: Point<Pixels>) {
        if self.lift.is_some() {
            return;
        }
        self.press = Some(Press { key, index, count, start, pulled: false });
    }

    /// Whether a thing is lifted, so that the press that lifted it isn't a click.
    pub fn is_lifted(&self) -> bool {
        self.lift.is_some()
    }

    pub fn lifted(&self) -> Option<usize> {
        self.lift.as_ref().map(|lift| lift.from)
    }

    fn moved(&mut self, position: Point<Pixels>) -> bool {
        let Some(press) = self.press.as_ref() else { return false };
        let (start, pulled) = (press.start, press.pulled);
        if !pulled {
            let (dx, dy) = (f32::from(position.x - start.x), f32::from(position.y - start.y));
            if dx.hypot(dy) < PULL {
                return false;
            }
            self.pick_up();
        }
        let pulled = self.along(position) - self.along(start);
        let Some(lift) = self.lift.as_mut().filter(|lift| lift.dropping.is_none()) else { return false };
        lift.follow(pulled);
        true
    }

    fn pick_up(&mut self) {
        let Some(press) = self.press.as_mut() else { return };
        press.pulled = true;
        let spans = self.spans.borrow();
        if press.count < 2 || spans.len() < press.count || press.index >= press.count {
            return;
        }
        let spans = spans[..press.count].to_vec();
        let shifts = vec![Tween::new(0., 0., SLIDE); spans.len()];
        self.lift = Some(Lift {
            key: press.key.clone(),
            from: press.index,
            index: press.index,
            spans,
            pulled: 0.,
            shifts,
            dropping: None,
        });
    }

    /// The press ended: a lifted thing slides into its place. `true` once it is on its way.
    fn released(&mut self) -> bool {
        self.press = None;
        let Some(lift) = self.lift.as_mut().filter(|lift| lift.dropping.is_none()) else { return false };
        lift.dropping = Some(Tween::new(lift.pulled, lift.landing(), DROP));
        true
    }

    /// The lifted thing is in its place: what was moved, and where to among the others.
    fn land(&mut self) -> Option<(K, usize)> {
        let lift = self.lift.take()?;
        (lift.index != lift.from).then_some((lift.key, lift.index))
    }

    /// How far the thing at `index` is drawn from where it is laid out.
    fn offset(&self, index: usize) -> f32 {
        let Some(lift) = self.lift.as_ref() else { return 0. };
        if index != lift.from {
            return lift.shifts.get(index).map_or(0., Tween::value);
        }
        lift.dropping.as_ref().map_or(lift.pulled, Tween::value)
    }

    /// The thing at `index`, slid out of the way of a lifted one, or lifted over its neighbours.
    pub fn place<E: Styled + IntoElement + 'static>(
        &self,
        index: usize,
        element: E,
        window: &mut Window,
    ) -> AnyElement {
        if self.lift.as_ref().is_some_and(|lift| lift.moving()) {
            window.request_animation_frame();
        }
        let offset = px(self.offset(index));
        let element = match self.axis {
            Axis::Horizontal => element.left(offset),
            Axis::Vertical => element.top(offset),
        };
        if self.lifted() == Some(index) {
            return deferred(element).with_priority(PRIORITY).into_any_element();
        }
        element.into_any_element()
    }

    /// While a thing is pressed, the window's every move and release goes to it, wherever the
    /// pointer is. A move without the button held ends a press whose release came before these
    /// listeners did. Once it has slid into its place, `moved` moves it.
    pub fn follow_pointer<V: 'static>(
        &self,
        view: WeakEntity<V>,
        reorder: fn(&mut V) -> &mut Self,
        moved: fn(&mut V, K, usize, &mut Context<V>),
    ) -> Option<AnyElement> {
        self.press.as_ref()?;
        let release = move |this: &mut V, cx: &mut Context<V>| {
            if reorder(this).released() {
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(DROP).await;
                    let _ = this.update(cx, |this, cx| {
                        if let Some((key, index)) = reorder(this).land() {
                            moved(this, key, index, cx);
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
            cx.notify();
        };
        Some(
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let view_moved = view.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let _ = view_moved.update(cx, |this, cx| {
                            if event.pressed_button != Some(MouseButton::Left) {
                                return release(this, cx);
                            }
                            if reorder(this).moved(event.position) {
                                cx.notify();
                            }
                        });
                    });
                    let view_released = view.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                            return;
                        }
                        let _ = view_released.update(cx, |this, cx| release(this, cx));
                    });
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        )
    }
}
