//! What core's scenes animate rather than paint (LAYOUT.md's Motion): Flashcard's flip and
//! Celebration's confetti, each a pure function of the time and a driver on GPUI's frames.

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::*;
use kurbo::{Affine, PathEl};
use motile_core::render::openui::canvas::{Color, Confetti, Flip};

use super::paint::{Palette, rounded_rect, tessellated};
use crate::media::native::reduces_motion;
use crate::theme::{Surface, colors};

/// Over the menus, the modals and the viewer.
const CONFETTI_PRIORITY: usize = 10_000;

/// How wide a turning face is drawn `seconds` into the flip, and whether the other face shows.
pub fn flip_scale(flip: &Flip, seconds: f32) -> (f32, bool) {
    let t = if flip.duration > 0. { (seconds / flip.duration).clamp(0., 1.) } else { 1. };
    let scale = match flip.frames.iter().position(|[at, _]| *at >= t) {
        None => flip.frames.last().map_or(1., |[_, scale]| *scale),
        Some(0) => flip.frames[0][1],
        Some(index) => {
            let ([from, low], [to, high]) = (flip.frames[index - 1], flip.frames[index]);
            if to <= from { high } else { low + (high - low) * (t - from) / (to - from) }
        }
    };
    (scale, t >= flip.swap_at)
}

/// A card's turn as it shows now: how wide its face is drawn, and whether that is the back.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Turn {
    pub scale: f32,
    pub back: bool,
}

struct Turning {
    back: bool,
    since: Option<Instant>,
}

/// The turn of the card `id`, which shows its back while `back`: it turns over on `flip` each
/// time `back` changes, and at once under reduced motion.
pub fn turned(id: impl Into<ElementId>, flip: &Flip, back: bool, window: &mut Window, cx: &mut App) -> Turn {
    let state = window.use_keyed_state(id, cx, |_, _| Turning { back, since: None });
    if state.read(cx).back != back {
        let since = (!reduces_motion()).then(Instant::now);
        state.update(cx, |turning, _| *turning = Turning { back, since });
    }
    let Some(since) = state.read(cx).since else { return Turn { scale: 1., back } };
    let seconds = since.elapsed().as_secs_f32();
    if seconds >= flip.duration {
        state.update(cx, |turning, _| turning.since = None);
        return Turn { scale: 1., back };
    }
    window.request_animation_frame();
    let (scale, past_swap) = flip_scale(flip, seconds);
    Turn { scale, back: if past_swap { back } else { !back } }
}

/// The face narrowed to `scale` of its width about its centre. GPUI can't scale what it draws, so
/// the face keeps its own width and shows through a window that narrows.
pub fn squashed(face: impl IntoElement, scale: f32) -> Div {
    let scale = scale.clamp(0., 1.);
    let inset = (1. - scale) / 2.;
    let through = div().absolute().top_0().bottom_0().left(relative(inset)).right(relative(inset)).overflow_hidden();
    let through = match scale > 0.001 {
        true => through.child(
            div().absolute().top_0().bottom_0().left(relative(-inset / scale)).w(relative(1. / scale)).child(face),
        ),
        false => through,
    };
    div().relative().size_full().child(through)
}

/// Where a piece of confetti is: its top left corner before it turns, its size and corners, how
/// far it has turned about its centre in degrees, and how much of it shows.
#[derive(Clone, PartialEq, Debug)]
pub struct PieceFrame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub turn: f32,
    pub opacity: f32,
    pub color: Color,
}

/// Every piece `seconds` into the burst, over a window `width` × `height`.
pub fn confetti_frames(confetti: &Confetti, width: f32, height: f32, seconds: f32) -> Vec<PieceFrame> {
    confetti
        .pieces
        .iter()
        .map(|piece| {
            let progress =
                if piece.duration > 0. { ((seconds - piece.delay) / piece.duration).clamp(0., 1.) } else { 1. };
            let eased = ease(confetti.curve, progress);
            PieceFrame {
                x: piece.x * width + piece.drift * eased,
                y: confetti.top + confetti.fall * height * eased,
                width: confetti.width,
                height: confetti.height,
                radius: confetti.radius,
                turn: piece.turn * eased,
                opacity: 1. + (confetti.end_opacity - 1.) * eased,
                color: piece.color.clone(),
            }
        })
        .collect()
}

/// A CSS timing function's progress at the moment `x`.
fn ease([x1, y1, x2, y2]: [f32; 4], x: f32) -> f32 {
    if x <= 0. || x >= 1. {
        return x.clamp(0., 1.);
    }
    let bezier = |a: f32, b: f32, t: f32| {
        let u = 1. - t;
        3. * u * u * t * a + 3. * u * t * t * b + t * t * t
    };
    let (mut low, mut high) = (0f32, 1f32);
    for _ in 0..40 {
        let middle = (low + high) / 2.;
        if bezier(x1, x2, middle) < x { low = middle } else { high = middle }
    }
    bezier(y1, y2, (low + high) / 2.)
}

/// Celebration's burst over the whole window, from `started`, which takes no input and is gone
/// once it has played; under reduced motion it draws nothing.
#[derive(IntoElement)]
pub struct ConfettiOverlay {
    confetti: Arc<Confetti>,
    started: Instant,
}

impl ConfettiOverlay {
    pub fn new(confetti: Arc<Confetti>, started: Instant) -> Self {
        Self { confetti, started }
    }
}

impl RenderOnce for ConfettiOverlay {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if reduces_motion() || self.started.elapsed().as_secs_f32() >= self.confetti.duration {
            return Empty.into_any_element();
        }
        window.request_animation_frame();
        let viewport = window.viewport_size();
        let palette = Palette::new(colors(cx), Surface::Background);
        let (confetti, started) = (self.confetti, self.started);
        let pieces = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| paint_confetti(&confetti, started, bounds, &palette, window),
        )
        .w(viewport.width)
        .h(viewport.height);
        let overlay =
            anchored().position_mode(AnchoredPositionMode::Window).position(point(px(0.), px(0.))).child(pieces);
        deferred(overlay).with_priority(CONFETTI_PRIORITY).into_any_element()
    }
}

fn paint_confetti(
    confetti: &Confetti,
    started: Instant,
    bounds: Bounds<Pixels>,
    palette: &Palette,
    window: &mut Window,
) {
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let seconds = started.elapsed().as_secs_f32();
    let fill = || PathStyle::Fill(FillOptions::non_zero());
    for piece in confetti_frames(confetti, width, height, seconds) {
        let (w, h, r) = (piece.width as f64, piece.height as f64, piece.radius as f64);
        let center = (
            f64::from(bounds.origin.x) + piece.x as f64 + w / 2.,
            f64::from(bounds.origin.y) + piece.y as f64 + h / 2.,
        );
        let placed = Affine::translate(center) * Affine::rotate((piece.turn as f64).to_radians());
        let outline: Vec<PathEl> = (placed * rounded_rect(-w / 2., -h / 2., w, h, r, r)).elements().to_vec();
        let Some(path) = tessellated(&outline, fill()) else { continue };
        window.paint_path(path, palette.resolve(&piece.color, piece.opacity));
    }
}
