//! Moving one of a row or a column of things to another place among them: pulled, or held when it
//! lifts by holding too, it leaves its gap and a copy of it floats where the pointer has it. It
//! lands in the slot whose middle is nearest its own, the others sliding out of that slot's way,
//! and let go it slides into it. A press that never pulled is a click: `RecycledListMac.swift`'s
//! `RecycledTable` and `RowLift`.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::*;

use super::ease_out;
use super::motion::Tween;
use crate::theme::{Layer, Opacity, Radius, ShadowSize, Surface, colors};

/// How long a press is held before what lifts by holding lifts, and how far it is pulled.
const HOLD: Duration = Duration::from_millis(200);
const PULL: f32 = 3.;
const SLIDE: Duration = Duration::from_millis(200);
const DROP: Duration = Duration::from_millis(250);
/// How much nearer another slot has to be than the one it would land in to be taken instead.
const NEARER: f32 = 4.;
/// Above the rows and the card, under the menus and the modals.
const PRIORITY: usize = 20;

/// Where the thing at `from` would start in `landing`, among the others, given where each thing
/// starts and how long it is.
fn slot_start(spans: &[(f32, f32)], from: usize, landing: usize) -> f32 {
    let (start, length) = spans[landing];
    if landing <= from { start } else { start + length - spans[from].1 }
}

/// The landing among `landings` whose slot's middle is nearest `middle`, the middle of the thing
/// lifted from `from`. It stays in `current` unless another is nearer by more than `NEARER`.
pub fn nearest_landing(
    spans: &[(f32, f32)],
    from: usize,
    landings: Range<usize>,
    current: usize,
    middle: f32,
) -> usize {
    let half = spans[from].1 / 2.;
    let distance = |landing: usize| (slot_start(spans, from, landing) + half - middle).abs();
    let Some(nearest) = landings.clone().min_by(|a, b| distance(*a).total_cmp(&distance(*b))) else { return current };
    if !landings.contains(&current) || distance(nearest) + NEARER < distance(current) { nearest } else { current }
}

/// What a list that reorders does on its owner `V`: where it is kept, what moving a thing to a
/// landing does, and what a press that never pulled does, when it doesn't take its own clicks.
pub struct Handlers<V, K> {
    pub reorder: fn(&mut V) -> &mut Reorder<K>,
    pub moved: fn(&mut V, K, usize, &mut Context<V>),
    pub clicked: Option<fn(&mut V, K, &mut Context<V>)>,
}

impl<V, K> Clone for Handlers<V, K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<V, K> Copy for Handlers<V, K> {}

/// The lifted thing's copy as it is drawn now: where, how big, and how far it has settled into
/// its slot, from 0 to 1.
pub struct Floating {
    pub origin: Point<Pixels>,
    pub size: Size<Pixels>,
    pub settled: f32,
}

/// The things that can be moved, of keys `K`, laid out along `axis`.
pub struct Reorder<K> {
    axis: Axis,
    /// It lifts when held, too.
    held: bool,
    /// Its copy follows the pointer every way and anywhere, not only along the things.
    free: bool,
    /// Where each thing was last laid out, and where that is measured from in the window.
    bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    frame: Rc<Cell<Point<Pixels>>>,
    press: Option<Press<K>>,
    lift: Option<Lift<K>>,
}

struct Press<K> {
    key: K,
    index: usize,
    landings: Range<usize>,
    /// Where it was pressed in the window.
    start: Point<Pixels>,
    pulled: bool,
    _hold: Option<Task<()>>,
}

struct Lift<K> {
    key: K,
    axis: Axis,
    from: usize,
    /// Where it would land, among the others, and where it can.
    index: usize,
    landings: Range<usize>,
    /// Where each thing starts along the axis and how long it is, and the lifted one's bounds.
    spans: Vec<(f32, f32)>,
    bounds: Bounds<Pixels>,
    /// Where the pointer pressed and is, from where the bounds are measured.
    start: Point<Pixels>,
    pointer: Point<Pixels>,
    shifts: Vec<Tween>,
    /// Where its copy slides from and to in the window as it lands, and since when.
    dropping: Option<(Point<Pixels>, Point<Pixels>, Instant)>,
}

fn along(axis: Axis, point: Point<Pixels>) -> f32 {
    f32::from(match axis {
        Axis::Horizontal => point.x,
        Axis::Vertical => point.y,
    })
}

fn moved_along(axis: Axis, point: Point<Pixels>, by: f32) -> Point<Pixels> {
    match axis {
        Axis::Horizontal => Point { x: point.x + px(by), y: point.y },
        Axis::Vertical => Point { x: point.x, y: point.y + px(by) },
    }
}

impl<K> Lift<K> {
    /// How far a thing is slid out of the way, with the gap where the lifted one would land.
    fn shift(&self, item: usize) -> f32 {
        let length = self.spans[self.from].1;
        if item > self.from && item <= self.index {
            -length
        } else if item < self.from && item >= self.index {
            length
        } else {
            0.
        }
    }

    /// How far it is pulled along the axis, kept between the first slot and the last.
    fn pulled(&self) -> f32 {
        let start = self.spans[self.from].0;
        let least = slot_start(&self.spans, self.from, self.landings.start) - start;
        let most = slot_start(&self.spans, self.from, self.landings.end - 1) - start;
        along(self.axis, self.pointer - self.start).clamp(least, most.max(least))
    }

    /// Follows the pointer, and parts the others where it would land.
    fn follow(&mut self, pointer: Point<Pixels>) {
        self.pointer = pointer;
        let (start, length) = self.spans[self.from];
        let middle = start + self.pulled() + length / 2.;
        let landing = nearest_landing(&self.spans, self.from, self.landings.clone(), self.index, middle);
        if landing == self.index {
            return;
        }
        self.index = landing;
        for item in 0..self.spans.len() {
            let target = self.shift(item);
            self.shifts[item].retarget(target);
        }
    }

    /// Where its copy is in the window, `frame` being where the bounds are measured from.
    fn origin(&self, free: bool, frame: Point<Pixels>) -> Point<Pixels> {
        if free {
            return self.bounds.origin + (self.pointer - self.start) + frame;
        }
        moved_along(self.axis, self.bounds.origin, self.pulled()) + frame
    }

    /// Where its slot is in the window.
    fn landing(&self, frame: Point<Pixels>) -> Point<Pixels> {
        let slot = slot_start(&self.spans, self.from, self.index) - self.spans[self.from].0;
        moved_along(self.axis, self.bounds.origin, slot) + frame
    }

    fn moving(&self) -> bool {
        self.dropping.is_some() || self.shifts.iter().any(Tween::moving)
    }
}

impl<K: Clone + 'static> Reorder<K> {
    pub fn new(axis: Axis) -> Self {
        Self { axis, held: false, free: false, bounds: Rc::default(), frame: Rc::default(), press: None, lift: None }
    }

    /// It lifts when held, too, not only when pulled.
    pub fn held(mut self) -> Self {
        self.held = true;
        self
    }

    /// Its copy follows the pointer every way and anywhere, not only along the things.
    pub fn free(mut self) -> Self {
        self.free = true;
        self
    }

    /// Keeps where the things are laid out in the window, for the `on_children_prepainted` of
    /// what holds them.
    pub fn measure(&self) -> impl Fn(Vec<Bounds<Pixels>>, &mut Window, &mut App) + 'static {
        let kept = self.bounds.clone();
        move |bounds, _, _| *kept.borrow_mut() = bounds
    }

    /// Keeps where the things are laid out in what scrolls with `handle`, whose children GPUI
    /// gives no `on_children_prepainted`, and as unscrolled.
    pub fn measure_scrolled(&self, handle: &ScrollHandle) {
        let scrolled = handle.offset();
        *self.bounds.borrow_mut() = (0..handle.children_count())
            .filter_map(|ix| handle.bounds_for_item(ix))
            .map(|bounds| Bounds { origin: bounds.origin + scrolled, size: bounds.size })
            .collect();
    }

    /// Keeps where the things of a list that only lays out what is in sight are, in its content.
    pub fn measure_content(&self, bounds: Vec<Bounds<Pixels>>) {
        *self.bounds.borrow_mut() = bounds;
    }

    /// Where the list's content starts in the window, as it scrolls.
    pub fn set_frame(&self, frame: Point<Pixels>) {
        self.frame.set(frame);
    }

    /// The thing at `index` was pressed at `start`. It lifts once it is pulled, or held when it
    /// lifts by holding, and can land among the others at `landings`, the things that move.
    pub fn press<V: 'static>(
        &mut self,
        key: K,
        index: usize,
        landings: Range<usize>,
        start: Point<Pixels>,
        handlers: Handlers<V, K>,
        cx: &mut Context<V>,
    ) {
        if self.lift.is_some() {
            return;
        }
        let hold = self.held.then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(HOLD).await;
                let _ = this.update(cx, |this, cx| {
                    (handlers.reorder)(this).pick_up();
                    cx.notify();
                });
            })
        });
        self.press = Some(Press { key, index, landings, start, pulled: false, _hold: hold });
        cx.notify();
    }

    pub fn is_pressed(&self) -> bool {
        self.press.is_some()
    }

    /// Whether a thing is lifted, so that the press that lifted it isn't a click.
    pub fn is_lifted(&self) -> bool {
        self.lift.is_some()
    }

    pub fn lifted(&self) -> Option<usize> {
        self.lift.as_ref().map(|lift| lift.from)
    }

    pub fn lifted_key(&self) -> Option<&K> {
        self.lift.as_ref().map(|lift| &lift.key)
    }

    /// Puts everything back where it is laid out, for when the things change under a lift.
    pub fn cancel(&mut self) {
        self.press = None;
        self.lift = None;
    }

    fn pick_up(&mut self) {
        if self.lift.is_some() {
            return;
        }
        let Some(press) = self.press.as_ref() else { return };
        let bounds = self.bounds.borrow();
        let landings = press.landings.clone();
        if landings.len() < 2 || landings.end > bounds.len() || !landings.contains(&press.index) {
            return;
        }
        let start = press.start - self.frame.get();
        let spans = bounds
            .iter()
            .map(|bounds| {
                let length = match self.axis {
                    Axis::Horizontal => bounds.size.width,
                    Axis::Vertical => bounds.size.height,
                };
                (along(self.axis, bounds.origin), f32::from(length))
            })
            .collect::<Vec<_>>();
        let shifts = vec![Tween::new(0., 0., SLIDE); spans.len()];
        self.lift = Some(Lift {
            key: press.key.clone(),
            axis: self.axis,
            from: press.index,
            index: press.index,
            landings,
            spans,
            bounds: bounds[press.index],
            start,
            pointer: start,
            shifts,
            dropping: None,
        });
    }

    fn moved(&mut self, position: Point<Pixels>) -> bool {
        let Some(press) = self.press.as_mut() else { return false };
        if !press.pulled {
            let (dx, dy) = (f32::from(position.x - press.start.x), f32::from(position.y - press.start.y));
            if dx.hypot(dy) < PULL {
                return false;
            }
            press.pulled = true;
            self.pick_up();
        }
        let frame = self.frame.get();
        let Some(lift) = self.lift.as_mut().filter(|lift| lift.dropping.is_none()) else { return false };
        lift.follow(position - frame);
        true
    }

    /// The lifted thing's copy sets off for its slot. `true` once it is on its way.
    fn drop_lifted(&mut self) -> bool {
        let frame = self.frame.get();
        let free = self.free;
        let Some(lift) = self.lift.as_mut().filter(|lift| lift.dropping.is_none()) else { return false };
        lift.dropping = Some((lift.origin(free, frame), lift.landing(frame), Instant::now()));
        true
    }

    /// The press ended, as a release when `click`: a lifted thing slides into its slot and is
    /// moved there once it has, and one that was never pulled is clicked.
    fn finish<V: 'static>(this: &mut V, handlers: Handlers<V, K>, click: bool, cx: &mut Context<V>) {
        let reorder = (handlers.reorder)(this);
        let Some(press) = reorder.press.take() else { return };
        if reorder.drop_lifted() {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(DROP).await;
                let _ = this.update(cx, |this, cx| {
                    let reorder = (handlers.reorder)(this);
                    let Some(lift) = reorder.lift.take() else { return };
                    if lift.index != lift.from {
                        (handlers.moved)(this, lift.key, lift.index, cx);
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        if click
            && !press.pulled
            && let Some(clicked) = handlers.clicked
        {
            clicked(this, press.key, cx);
        }
        cx.notify();
    }

    /// The press was released over what was pressed.
    pub fn release<V: 'static>(this: &mut V, handlers: Handlers<V, K>, cx: &mut Context<V>) {
        Self::finish(this, handlers, true, cx);
    }

    /// How far the thing at `index` is slid out of the way of a lifted one: `None` for the lifted
    /// one, whose gap is left empty.
    pub fn shift(&self, index: usize) -> Option<f32> {
        let Some(lift) = self.lift.as_ref() else { return Some(0.) };
        if index == lift.from {
            return None;
        }
        Some(lift.shifts.get(index).map_or(0., Tween::value))
    }

    /// The thing at `index`, slid out of the way of a lifted one, or hidden where the lifted
    /// one leaves its gap.
    pub fn place<E: Styled + IntoElement + 'static>(&self, index: usize, element: E) -> AnyElement {
        let Some(shift) = self.shift(index) else { return element.invisible().into_any_element() };
        match self.axis {
            Axis::Horizontal => element.left(px(shift)).into_any_element(),
            Axis::Vertical => element.top(px(shift)).into_any_element(),
        }
    }

    /// Where the lifted thing's copy is drawn now, asking for the next frame while anything moves.
    pub fn floating(&self, window: &mut Window) -> Option<Floating> {
        let lift = self.lift.as_ref()?;
        if lift.moving() {
            window.request_animation_frame();
        }
        let size = lift.bounds.size;
        let Some((from, to, started)) = lift.dropping else {
            return Some(Floating { origin: lift.origin(self.free, self.frame.get()), size, settled: 0. });
        };
        let settled = ease_out((started.elapsed().as_secs_f32() / DROP.as_secs_f32()).min(1.));
        let origin = point(from.x + (to.x - from.x) * settled, from.y + (to.y - from.y) * settled);
        Some(Floating { origin, size, settled })
    }

    /// `copy` floating over everything in the window at `origin`.
    pub fn float(origin: Point<Pixels>, copy: impl IntoElement) -> AnyElement {
        deferred(anchored().position(origin).child(copy)).with_priority(PRIORITY).into_any_element()
    }

    /// The lifted thing's copy: `copy` on a lit, shadowed light, `inset` as the thing's own light
    /// is. It takes the pointer, so what it passes over neither lights up nor shows its tooltip.
    pub fn layer(
        &self,
        copy: impl IntoElement,
        inset: Edges<f32>,
        window: &mut Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let floating = self.floating(window)?;
        let c = colors(cx);
        let light = div()
            .absolute()
            .top(px(inset.top))
            .left(px(inset.left))
            .bottom(px(inset.bottom))
            .right(px(inset.right))
            .rounded(px(Radius::MD))
            .bg(Surface::Background.color(c))
            .shadow(c.shadow(ShadowSize::Md, Opacity::Shadow))
            .child(div().size_full().rounded(px(Radius::MD)).bg(Surface::Background.layer(Layer::ControlLit, c)));
        let lifted = div()
            .id("reorder-lifted")
            .occlude()
            .relative()
            .w(floating.size.width)
            .h(floating.size.height)
            .child(light)
            .child(copy);
        Some(Self::float(floating.origin, lifted))
    }

    /// While a thing is pressed, the window's every move and release goes to it, wherever the
    /// pointer is. A move without the button held ends a press whose release came before these
    /// listeners did, as no click.
    pub fn follow_pointer<V: 'static>(&self, view: WeakEntity<V>, handlers: Handlers<V, K>) -> Option<AnyElement> {
        self.press.as_ref()?;
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
                                return Self::finish(this, handlers, false, cx);
                            }
                            if (handlers.reorder)(this).moved(event.position) {
                                cx.notify();
                            }
                        });
                    });
                    let view_released = view.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble || event.button != MouseButton::Left {
                            return;
                        }
                        let _ = view_released.update(cx, |this, cx| Self::release(this, handlers, cx));
                    });
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::nearest_landing;

    /// Lands the thing lifted from `from` with its middle at each of `middles` in turn.
    fn landings(spans: &[(f32, f32)], from: usize, middles: &[f32]) -> Vec<usize> {
        let mut current = from;
        middles
            .iter()
            .map(|middle| {
                current = nearest_landing(spans, from, 0..spans.len(), current, *middle);
                current
            })
            .collect()
    }

    #[test]
    fn things_of_a_size_swap_halfway_across_not_a_whole_length_on() {
        let spans = [(0., 34.), (34., 34.), (68., 34.)];
        assert_eq!(landings(&spans, 0, &[17. + 16., 17. + 22.]), vec![0, 1]);
        assert_eq!(landings(&spans, 0, &[17. + 22. + 34.]), vec![2]);
    }

    #[test]
    fn a_small_thing_over_a_big_one_swaps_at_half_the_big_ones_length_and_stays() {
        let spans = [(0., 20.), (20., 100.)];
        assert_eq!(landings(&spans, 0, &[10. + 48.]), vec![0]);
        assert_eq!(landings(&spans, 0, &[10. + 53., 10. + 50., 10. + 48.]), vec![1, 1, 1]);
    }

    #[test]
    fn another_slot_is_taken_only_once_it_is_nearer_by_more_than_four_points() {
        let spans = [(0., 34.), (34., 34.)];
        assert_eq!(nearest_landing(&spans, 0, 0..2, 0, 17. + 19.), 0);
        assert_eq!(nearest_landing(&spans, 0, 0..2, 0, 17. + 19.5), 1);
        assert_eq!(nearest_landing(&spans, 0, 0..2, 1, 17. + 15.), 1);
        assert_eq!(nearest_landing(&spans, 0, 0..2, 1, 17. + 14.5), 0);
    }

    #[test]
    fn it_lands_only_among_the_things_that_move() {
        let spans = [(0., 30.), (30., 30.), (60., 30.), (90., 30.), (120., 30.)];
        assert_eq!(nearest_landing(&spans, 2, 1..4, 2, -100.), 1);
        assert_eq!(nearest_landing(&spans, 2, 1..4, 2, 500.), 3);
        assert_eq!(nearest_landing(&spans, 2, 1..3, 5, 75.), 2);
    }
}
