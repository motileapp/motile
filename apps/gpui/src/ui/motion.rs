//! What moves, as SwiftUI's animations and transitions do: a value that eases to where it is
//! going from where it is, and what is kept a moment after it went, so that it can go out as it
//! came in.

use std::time::{Duration, Instant};

use gpui_kit::*;

use crate::theme::{Opacity, colors};
use crate::ui::ease_out;

/// SwiftUI's `.easeOut(duration: 0.12)`, which choices, marks and dropdowns change with.
pub const QUICK: Duration = Duration::from_millis(120);
/// How long a modal takes to come and to go.
pub const MODAL: Duration = Duration::from_millis(180);

/// A value on its way: where it set off from, where it goes, and when.
#[derive(Clone, Copy)]
pub struct Tween {
    from: f32,
    to: f32,
    since: Instant,
    duration: Duration,
    curve: fn(f32) -> f32,
}

impl Tween {
    pub fn new(from: f32, to: f32, duration: Duration) -> Self {
        Self { from, to, since: Instant::now(), duration, curve: ease_out }
    }

    pub fn curve(mut self, curve: fn(f32) -> f32) -> Self {
        self.curve = curve;
        self
    }

    pub fn target(&self) -> f32 {
        self.to
    }

    pub fn value(&self) -> f32 {
        let elapsed = self.since.elapsed().as_secs_f32() / self.duration.as_secs_f32().max(f32::EPSILON);
        if elapsed >= 1. {
            return self.to;
        }
        self.from + (self.to - self.from) * (self.curve)(elapsed)
    }

    pub fn moving(&self) -> bool {
        self.value() != self.to
    }

    /// Sets off for `to` from where it is now.
    pub fn retarget(&mut self, to: f32) {
        if to == self.to {
            return;
        }
        *self = Self { from: self.value(), to, since: Instant::now(), ..*self };
    }
}

/// A value that eases to `target` over `duration` whenever it changes. It starts at `first`, so
/// `first` other than `target` makes it come in.
pub fn eased_from(
    id: impl Into<ElementId>,
    first: f32,
    target: f32,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    eased_with(id, target, Tween::new(first, target, duration), window, cx)
}

/// A value that eases to `target` over `duration` whenever it changes, first at `target`.
pub fn eased(id: impl Into<ElementId>, target: f32, duration: Duration, window: &mut Window, cx: &mut App) -> f32 {
    eased_from(id, target, target, duration, window, cx)
}

/// `eased_from` on another curve.
pub fn eased_on(
    id: impl Into<ElementId>,
    first: f32,
    target: f32,
    duration: Duration,
    curve: fn(f32) -> f32,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    eased_with(id, target, Tween::new(first, target, duration).curve(curve), window, cx)
}

fn eased_with(id: impl Into<ElementId>, target: f32, start: Tween, window: &mut Window, cx: &mut App) -> f32 {
    let state = window.use_keyed_state(id, cx, |_, _| start);
    if state.read(cx).target() != target {
        state.update(cx, |tween, _| tween.retarget(target));
    }
    let tween = *state.read(cx);
    if tween.moving() {
        window.request_animation_frame();
    }
    tween.value()
}

/// The item while there is one, and the last one for `duration` after it went, so that what
/// shows it can go out. The flag says it is going.
pub fn lingering<T: Clone + 'static>(
    id: impl Into<ElementId>,
    item: Option<T>,
    duration: Duration,
    window: &mut Window,
    cx: &mut App,
) -> Option<(T, bool)> {
    let state = window.use_keyed_state(id, cx, |_, _| None::<(T, Option<Instant>)>);
    if let Some(item) = item {
        state.update(cx, |kept, _| *kept = Some((item.clone(), None)));
        return Some((item, false));
    }
    let (last, went) = state.read(cx).clone()?;
    let went = went.unwrap_or_else(Instant::now);
    if went.elapsed() >= duration {
        state.update(cx, |kept, _| *kept = None);
        return None;
    }
    state.update(cx, |kept, _| *kept = Some((last.clone(), Some(went))));
    window.request_animation_frame();
    Some((last, true))
}

/// How long a placeholder takes to fade and come back.
const PULSE: Duration = Duration::from_millis(800);

/// What stands in for what is yet to come, fading in and out.
pub fn pulsing<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    element: E,
    cx: &App,
) -> AnimationElement<E> {
    let faded = colors(cx).opacity(Opacity::Disabled);
    element.with_animation(
        id,
        Animation::new(PULSE).repeat().with_easing(bounce(gpui_kit::ease_in_out)),
        move |element, delta| element.opacity(1. - (1. - faded) * delta),
    )
}

/// How long SwiftUI's default animation takes to come to rest: a spring without bounce.
pub const SPRING: Duration = Duration::from_millis(600);

/// SwiftUI's default animation, a critically damped spring of response 0.55, over `SPRING`.
pub fn spring(delta: f32) -> f32 {
    let omega = std::f32::consts::TAU / 0.55;
    let at = |t: f32| 1. - (1. + omega * t) * (-omega * t).exp();
    at(delta * SPRING.as_secs_f32()) / at(SPRING.as_secs_f32())
}

/// SwiftUI's ease-in-out curve, near enough.
pub fn ease_in_out(delta: f32) -> f32 {
    if delta < 0.5 { 2. * delta * delta } else { 1. - (-2. * delta + 2.).powi(2) / 2. }
}

/// CSS's `cubic-bezier(x1, y1, x2, y2)` at `t` of the way through: the curve's height where its
/// horizontal position is `t`, found by Newton's method with bisection where that stalls.
pub fn cubic_bezier(first: (f32, f32), second: (f32, f32), t: f32) -> f32 {
    let along = |a: f32, b: f32, s: f32| 3. * a * s * (1. - s).powi(2) + 3. * b * s * s * (1. - s) + s.powi(3);
    let slope =
        |a: f32, b: f32, s: f32| 3. * a * (1. - s).powi(2) + 6. * (b - a) * s * (1. - s) + 3. * (1. - b) * s * s;
    let t = t.clamp(0., 1.);
    let mut s = t;
    for _ in 0..8 {
        let error = along(first.0, second.0, s) - t;
        let gradient = slope(first.0, second.0, s);
        if error.abs() < 1e-5 || gradient.abs() < 1e-6 {
            break;
        }
        s = (s - error / gradient).clamp(0., 1.);
    }
    if (along(first.0, second.0, s) - t).abs() > 1e-3 {
        let (mut low, mut high) = (0., 1.);
        for _ in 0..30 {
            s = (low + high) / 2.;
            if along(first.0, second.0, s) < t { low = s } else { high = s }
        }
    }
    along(first.1, second.1, s)
}

/// The colour `t` of the way from `from` to `to`.
pub fn mix(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let (from, to) = (Rgba::from(from), Rgba::from(to));
    let at = |a: f32, b: f32| a + (b - a) * t;
    Hsla::from(Rgba { r: at(from.r, to.r), g: at(from.g, to.g), b: at(from.b, to.b), a: at(from.a, to.a) })
}

#[cfg(test)]
mod tests {
    use super::cubic_bezier;

    #[test]
    fn a_cubic_bezier_runs_from_start_to_end_along_its_curve() {
        assert_eq!(cubic_bezier((0.16, 1.), (0.3, 1.), 0.), 0.);
        assert!((cubic_bezier((0.16, 1.), (0.3, 1.), 1.) - 1.).abs() < 1e-4);
        assert!((cubic_bezier((1. / 3., 1. / 3.), (2. / 3., 2. / 3.), 0.3) - 0.3).abs() < 1e-3);
        assert!(cubic_bezier((0.16, 1.), (0.3, 1.), 0.25) > 0.75);
    }
}
