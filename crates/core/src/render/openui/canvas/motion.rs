//! Celebration's confetti and Flashcard's flip, as open-intelligent-ui's `extra.tsx` and
//! `components.css` animate them.

use super::scene::{Color, Confetti, Flip, Motion, Piece, Scene};

const PIECES: usize = 70;
const CONFETTI_CURVE: [f32; 4] = [0.2, 0.6, 0.4, 1.0];
const FLIP_CURVE: [f32; 4] = [0.3, 0.7, 0.3, 1.0];
const FLIP_SECONDS: f32 = 0.45;
/// The flip's frames, so many a second.
const FLIP_RATE: f32 = 60.0;

/// The burst Celebration fires.
pub fn celebration() -> Scene {
    let pieces = (0..PIECES)
        .map(|i| Piece {
            x: ((i * 37) % 100) as f32 / 100.0,
            color: Color::Token(format!("chart-{}", i % 6 + 1).into()),
            delay: ((i % 10) * 40) as f32 / 1000.0,
            duration: (1200 + (i * 53) % 900) as f32 / 1000.0,
            drift: ((i * 71) % 200) as f32 - 100.0,
            turn: ((i * 97) % 720) as f32,
        })
        .collect();
    let confetti = Confetti {
        duration: 2.2,
        curve: CONFETTI_CURVE,
        width: 8.0,
        height: 12.0,
        radius: 2.0,
        top: -12.0,
        fall: 1.05,
        end_opacity: 0.7,
        pieces,
    };
    Scene { motion: Some(Motion::Confetti(confetti)), ..Scene::default() }
}

/// Flashcard's turn: the face narrows to nothing about its centre, the other widens from it.
pub fn flip() -> Scene {
    let swap_at = time_at(FLIP_CURVE, 0.5);
    let steps = (FLIP_SECONDS * FLIP_RATE).round() as usize;
    let mut moments: Vec<f32> = (0..=steps).map(|step| step as f32 / steps as f32).collect();
    let at = moments.partition_point(|t| *t < swap_at);
    moments.insert(at, swap_at);
    moments.dedup();
    let frames = moments
        .into_iter()
        .map(|t| {
            let progress = if t == swap_at { 0.5 } else { ease(FLIP_CURVE, t) };
            [round3(t), round3((1.0 - 2.0 * progress).abs())]
        })
        .collect();
    let flip = Flip { duration: FLIP_SECONDS, curve: FLIP_CURVE, swap_at: round3(swap_at), frames };
    Scene { motion: Some(Motion::Flip(flip)), ..Scene::default() }
}

fn round3(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

/// The point of a cubic Bézier from (0, 0) to (1, 1) at `t`, given its two controls.
fn bezier(a: f32, b: f32, t: f32) -> f32 {
    let u = 1.0 - t;
    3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
}

/// The timing function's progress at the moment `x`, as CSS computes it.
fn ease(curve: [f32; 4], x: f32) -> f32 {
    bezier(curve[1], curve[3], parameter(curve, x))
}

/// The moment the timing function reaches `progress`.
fn time_at(curve: [f32; 4], progress: f32) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let middle = (lo + hi) / 2.0;
        if ease(curve, middle) < progress { lo = middle } else { hi = middle }
    }
    (lo + hi) / 2.0
}

/// The curve's parameter where its x is `x`, by bisection: x grows with it.
fn parameter(curve: [f32; 4], x: f32) -> f32 {
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let middle = (lo + hi) / 2.0;
        if bezier(curve[0], curve[2], middle) < x { lo = middle } else { hi = middle }
    }
    (lo + hi) / 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eases_as_css_does() {
        assert!((ease(FLIP_CURVE, 0.0)).abs() < 1e-4);
        assert!((ease(FLIP_CURVE, 1.0) - 1.0).abs() < 1e-4);
        assert!((ease([0.25, 0.25, 0.75, 0.75], 0.3) - 0.3).abs() < 1e-4);
        assert!(ease(FLIP_CURVE, 0.2) > 0.4);
    }

    #[test]
    fn the_flip_squashes_to_nothing_where_the_faces_swap() {
        let Some(Motion::Flip(flip)) = flip().motion else { panic!() };
        assert_eq!(flip.frames.first(), Some(&[0.0, 1.0]));
        assert_eq!(flip.frames.last(), Some(&[1.0, 1.0]));
        assert!(flip.frames.contains(&[flip.swap_at, 0.0]));
        assert!(flip.swap_at > 0.1 && flip.swap_at < 0.3);
    }
}
