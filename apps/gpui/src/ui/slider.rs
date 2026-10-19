//! The slider: a value between two ends, picked on a round track by dragging its thumb, pressing
//! where it goes or stepping it with the arrow keys.

#![allow(dead_code, reason = "a component offers what the Mac app's does, whether a view uses it yet or not")]

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, Layer, Opacity, Surface, colors};
use crate::ui::menu::Anchor;

type OnValue = Rc<dyn Fn(f64, &mut Window, &mut App)>;
type OnEditing = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// The ends of a slider and the step its values snap to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Range {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl Range {
    /// Without a step, a hundredth of the way from one end to the other.
    pub fn new(min: f64, max: f64, step: Option<f64>) -> Self {
        let step = step.filter(|step| *step > 0.).unwrap_or((max - min) / 100.);
        Self { min, max, step }
    }

    /// How far along `value` lies, from 0 to 1.
    pub fn fraction(&self, value: f64) -> f64 {
        if self.max <= self.min {
            return 0.;
        }
        ((value - self.min) / (self.max - self.min)).clamp(0., 1.)
    }

    /// The value on the step nearest `value`, counted from `min`, held to the ends.
    pub fn snapped(&self, value: f64) -> f64 {
        if self.max <= self.min {
            return self.min;
        }
        if self.step <= 0. {
            return value.clamp(self.min, self.max);
        }
        let steps = ((value - self.min) / self.step).round();
        (self.min + steps * self.step).clamp(self.min, self.max)
    }

    /// The value `t` of the way along, snapped.
    pub fn at(&self, t: f64) -> f64 {
        self.snapped(self.min + t.clamp(0., 1.) * (self.max - self.min))
    }

    /// The value `steps` steps from `value`.
    pub fn stepped(&self, value: f64, steps: f64) -> f64 {
        self.snapped(value + steps * self.step)
    }
}

/// Where a mark's title stands against its place on the track.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkPlace {
    /// From the slider's start, for a first mark at its start.
    Leading,
    Centered,
    /// Up to the slider's end, for a last mark at its end.
    Trailing,
}

impl MarkPlace {
    pub fn of(index: usize, count: usize, t: f64) -> Self {
        if index == 0 && t <= 0.02 {
            return MarkPlace::Leading;
        }
        if index + 1 == count && t >= 0.98 {
            return MarkPlace::Trailing;
        }
        MarkPlace::Centered
    }
}

/// What is being dragged, by the slider's id, so that a drag moves only its own thumb.
#[derive(Clone)]
struct SliderDrag(ElementId);

impl Render for SliderDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// A value between `min` and `max`, with an optional label and readout over it and titled marks
/// under it. It fills its width. While dragged it shows its own value, and says when a drag starts
/// and ends with `on_editing`.
#[derive(IntoElement)]
pub struct Slider {
    id: ElementId,
    value: f64,
    range: Range,
    label: Option<SharedString>,
    readout: Option<SharedString>,
    marks: Vec<(f64, SharedString)>,
    surface: Surface,
    disabled: bool,
    on_change: Option<OnValue>,
    on_editing: Option<OnEditing>,
}

impl Slider {
    /// The band that takes presses, around the track.
    pub const CONTROL: f32 = ControlSize::Large.height();
    /// How far the thumb's centre stays from the ends.
    const INSET: f32 = 12.;
    const THUMB: f32 = 18.;
    const TRACK: f32 = ControlSize::Small.height();
    const LABEL_LINE: f32 = 20.;
    const READOUT_LINE: f32 = 18.;
    const MARKS: f32 = 18.;
    const MARKS_OVERLAP: f32 = 2.;

    pub fn new(id: impl Into<ElementId>, value: f64, min: f64, max: f64) -> Self {
        Self {
            id: id.into(),
            value,
            range: Range::new(min, max, None),
            label: None,
            readout: None,
            marks: Vec::new(),
            surface: Surface::Background,
            disabled: false,
            on_change: None,
            on_editing: None,
        }
    }

    /// The step its values snap to; without one, a hundredth of the range.
    pub fn step(mut self, step: Option<f64>) -> Self {
        self.range = Range::new(self.range.min, self.range.max, step);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The value as words, at the end of the head.
    pub fn readout(mut self, readout: impl Into<SharedString>) -> Self {
        self.readout = Some(readout.into());
        self
    }

    pub fn marks(mut self, marks: Vec<(f64, SharedString)>) -> Self {
        self.marks = marks;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.surface = surface;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Called with each new value while it is dragged, pressed or stepped.
    pub fn on_change(mut self, handler: impl Fn(f64, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }

    /// Called with true when a drag starts and false when it ends.
    pub fn on_editing(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_editing = Some(Rc::new(handler));
        self
    }

    /// How tall it is: the label's wrapped height, or the readout's line alone, the control, and
    /// the marks.
    pub fn height(label_height: Option<f32>, readout: bool, marks: bool) -> f32 {
        let head = match (label_height, readout) {
            (Some(height), _) => height,
            (None, true) => Self::READOUT_LINE,
            (None, false) => 0.,
        };
        head + Self::CONTROL + if marks { Self::MARKS - Self::MARKS_OVERLAP } else { 0. }
    }

    fn head(&self, cx: &App) -> Option<Div> {
        if self.label.is_none() && self.readout.is_none() {
            return None;
        }
        let c = colors(cx);
        let label = self.label.clone().map(|label| {
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(13.))
                .line_height(px(Self::LABEL_LINE))
                .text_color(c.foreground)
                .child(label)
        });
        let readout = self.readout.clone().map(|readout| {
            div()
                .flex_shrink_0()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .font_features(crate::ui::tabular_numbers())
                .line_height(px(Self::READOUT_LINE))
                .whitespace_nowrap()
                .text_color(c.foreground)
                .child(readout)
        });
        Some(div().w_full().flex().items_baseline().justify_end().gap(px(12.)).children(label).children(readout))
    }

    fn marks_row(&self, cx: &App) -> Option<Div> {
        if self.marks.is_empty() {
            return None;
        }
        let c = colors(cx);
        let count = self.marks.len();
        let titles = self.marks.iter().enumerate().map(|(index, (value, title))| {
            let t = self.range.fraction(*value);
            let text = div()
                .flex_shrink_0()
                .text_size(px(11.))
                .font_features(crate::ui::tabular_numbers())
                .line_height(px(14.))
                .whitespace_nowrap()
                .text_color(c.muted_foreground)
                .child(title.clone());
            let placed = div().absolute().top_0().h(px(Self::MARKS)).flex().items_center();
            match MarkPlace::of(index, count, t) {
                MarkPlace::Leading => placed.left_0().child(text),
                MarkPlace::Trailing => placed.right_0().child(text),
                MarkPlace::Centered => div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(Self::INSET))
                    .right(px(Self::INSET))
                    .child(placed.left(relative(t as f32)).w_0().justify_center().child(text)),
            }
        });
        Some(div().relative().w_full().h(px(Self::MARKS)).mt(px(-Self::MARKS_OVERLAP)).children(titles))
    }
}

/// The value at the pointer's `x` on a control drawn in `bounds`.
fn value_at(range: Range, x: Pixels, bounds: Bounds<Pixels>) -> f64 {
    let travel = f32::from(bounds.size.width) - 2. * Slider::INSET;
    if travel <= 0. {
        return range.min;
    }
    range.at(f64::from((f32::from(x - bounds.left()) - Slider::INSET) / travel))
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let disabled = self.disabled || crate::ui::inert(cx);
        let id = self.id.clone();
        let named = |name: &'static str| ElementId::NamedChild(std::sync::Arc::new(id.clone()), name.into());
        let dragged = window.use_keyed_state(named("dragged"), cx, |_, _| None::<f64>);
        let anchor = window.use_keyed_state(named("bounds"), cx, |_, _| Anchor::default()).read(cx).clone();
        let hovered = crate::ui::hover_state(&id, window, cx);
        let focus = crate::ui::focus_state(&id, window, cx);
        let range = self.range;
        let value = dragged.read(cx).unwrap_or(self.value);
        let t = range.fraction(value) as f32;
        let lit = !disabled && (*hovered.read(cx) || dragged.read(cx).is_some());
        let ring = !disabled && crate::ui::focus_shown(&focus, window);
        let head = self.head(cx);
        let marks = self.marks_row(cx);
        let c = colors(cx);
        let track_radius = Self::TRACK / 2.;
        let track_top = (Self::CONTROL - Self::TRACK) / 2.;
        let track = div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(track_top))
            .h(px(Self::TRACK))
            .rounded(px(track_radius))
            .bg(self.surface.layer(if lit { Layer::ControlLit } else { Layer::Control }, c))
            .when(ring, |track| track.child(crate::ui::focus_ring(track_radius, cx)));
        let thumb = div().absolute().top_0().bottom_0().left(px(Self::INSET)).right(px(Self::INSET)).child(
            div().absolute().top(px((Self::CONTROL - Self::THUMB) / 2.)).left(relative(t)).child(
                div().absolute().left(px(-Self::THUMB / 2.)).size(px(Self::THUMB)).rounded_full().bg(c.foreground),
            ),
        );
        let (on_change, on_editing) = (self.on_change, self.on_editing);
        let change = move |value: f64, window: &mut Window, cx: &mut App| {
            let Some(on_change) = &on_change else { return };
            on_change(value, window, cx)
        };
        let change = Rc::new(change);
        let editing = move |editing: bool, window: &mut Window, cx: &mut App| {
            let Some(on_editing) = &on_editing else { return };
            on_editing(editing, window, cx)
        };
        let editing = Rc::new(editing);
        let control = div()
            .id(named("control"))
            .relative()
            .w_full()
            .h(px(Self::CONTROL))
            .role(Role::Slider)
            .when_some(self.label.clone(), |control, label| control.aria_label(label))
            .when_some(self.readout.clone(), |control, readout| control.aria_value(readout))
            .aria_numeric_value(value)
            .aria_min_numeric_value(range.min)
            .aria_max_numeric_value(range.max)
            .aria_numeric_value_step(range.step)
            .child(anchor.track())
            .child(track)
            .child(thumb)
            .on_hover(crate::ui::track_hover(&hovered))
            .when(!disabled, |control| {
                let press = {
                    let (dragged, anchor, focus, change, editing) =
                        (dragged.clone(), anchor.clone(), focus.clone(), change.clone(), editing.clone());
                    move |event: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                        cx.stop_propagation();
                        window.focus(&focus, cx);
                        let value = value_at(range, event.position.x, anchor.bounds());
                        dragged.update(cx, |held, cx| {
                            *held = Some(value);
                            cx.notify();
                        });
                        editing(true, window, cx);
                        change(value, window, cx);
                    }
                };
                let drag = {
                    let (dragged, change, id) = (dragged.clone(), change.clone(), id.clone());
                    move |event: &DragMoveEvent<SliderDrag>, window: &mut Window, cx: &mut App| {
                        if event.drag(cx).0 != id || dragged.read(cx).is_none() {
                            return;
                        }
                        let value = value_at(range, event.event.position.x, event.bounds);
                        if *dragged.read(cx) == Some(value) {
                            return;
                        }
                        dragged.update(cx, |held, cx| {
                            *held = Some(value);
                            cx.notify();
                        });
                        change(value, window, cx);
                    }
                };
                let release = {
                    let (dragged, editing) = (dragged.clone(), editing.clone());
                    move |_: &MouseUpEvent, window: &mut Window, cx: &mut App| {
                        if dragged.read(cx).is_none() {
                            return;
                        }
                        dragged.update(cx, |held, cx| {
                            *held = None;
                            cx.notify();
                        });
                        editing(false, window, cx);
                    }
                };
                let release = Rc::new(release);
                let (up, up_out) = (release.clone(), release);
                let keys = {
                    let change = change.clone();
                    move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                        let next = match event.keystroke.key.as_str() {
                            "left" | "down" => range.stepped(value, -1.),
                            "right" | "up" => range.stepped(value, 1.),
                            "home" => range.min,
                            "end" => range.max,
                            _ => return,
                        };
                        cx.stop_propagation();
                        window.refresh();
                        if next != value {
                            change(next, window, cx);
                        }
                    }
                };
                let (increment, decrement) = (change.clone(), change.clone());
                control
                    .track_focus(&focus)
                    .on_mouse_down(MouseButton::Left, press)
                    .on_drag(SliderDrag(id.clone()), |drag, _, _, cx| cx.new(|_| drag.clone()))
                    .on_drag_move(drag)
                    .on_mouse_up(MouseButton::Left, move |event, window, cx| up(event, window, cx))
                    .on_mouse_up_out(MouseButton::Left, move |event, window, cx| up_out(event, window, cx))
                    .on_key_down(keys)
                    .on_a11y_action(AccessibleAction::Increment, move |_, window, cx| {
                        increment(range.stepped(value, 1.), window, cx)
                    })
                    .on_a11y_action(AccessibleAction::Decrement, move |_, window, cx| {
                        decrement(range.stepped(value, -1.), window, cx)
                    })
            });
        div()
            .id(id.clone())
            .w_full()
            .flex()
            .flex_col()
            .cursor_default()
            .when(disabled, |slider| slider.opacity(c.opacity(Opacity::Disabled)))
            .children(head)
            .child(control)
            .children(marks)
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use super::{MarkPlace, Range, Slider, value_at};

    #[test]
    fn snaps_to_the_nearest_step_from_min_and_holds_to_the_ends() {
        let range = Range::new(1., 11., Some(2.));
        assert_eq!(range.snapped(4.2), 5.);
        assert_eq!(range.snapped(3.9), 3.);
        assert_eq!(range.snapped(-4.), 1.);
        assert_eq!(range.snapped(40.), 11.);
        assert_eq!(range.stepped(5., 1.), 7.);
        assert_eq!(range.stepped(11., 1.), 11.);
    }

    #[test]
    fn steps_a_hundredth_without_a_step() {
        let range = Range::new(0., 50., None);
        assert_eq!(range.step, 0.5);
        assert_eq!(range.at(0.333), 16.5);
        assert_eq!(Range::new(0., 50., Some(0.)).step, 0.5);
    }

    #[test]
    fn fraction_is_held_between_zero_and_one() {
        let range = Range::new(10., 20., None);
        assert_eq!(range.fraction(15.), 0.5);
        assert_eq!(range.fraction(5.), 0.);
        assert_eq!(range.fraction(25.), 1.);
        assert_eq!(Range::new(3., 3., None).fraction(3.), 0.);
    }

    #[test]
    fn the_pointer_maps_onto_the_thumbs_travel() {
        let range = Range::new(0., 100., Some(1.));
        let bounds = Bounds::new(point(px(100.), px(0.)), size(px(224.), px(36.)));
        assert_eq!(value_at(range, px(112.), bounds), 0.);
        assert_eq!(value_at(range, px(212.), bounds), 50.);
        assert_eq!(value_at(range, px(400.), bounds), 100.);
    }

    #[test]
    fn end_marks_hug_the_ends() {
        assert_eq!(MarkPlace::of(0, 3, 0.), MarkPlace::Leading);
        assert_eq!(MarkPlace::of(0, 3, 0.1), MarkPlace::Centered);
        assert_eq!(MarkPlace::of(1, 3, 0.5), MarkPlace::Centered);
        assert_eq!(MarkPlace::of(2, 3, 1.), MarkPlace::Trailing);
    }

    #[test]
    fn height_adds_the_head_and_the_marks() {
        assert_eq!(Slider::height(None, false, false), 36.);
        assert_eq!(Slider::height(None, true, false), 54.);
        assert_eq!(Slider::height(Some(40.), true, true), 92.);
    }
}
