use std::f32::consts::TAU;

use motile_core::render::openui::canvas::{Region, Scene};

/// The hit under the point, in the scene's points: the last one that holds it.
pub fn hit(scene: &Scene, x: f32, y: f32) -> Option<usize> {
    scene.hits.iter().rposition(|hit| holds(&hit.region, x, y))
}

fn holds(region: &Region, x: f32, y: f32) -> bool {
    match *region {
        Region::Rect { x: left, y: top, width, height } => {
            x >= left && x <= left + width && y >= top && y <= top + height
        }
        Region::Circle { cx, cy, r } => (x - cx).hypot(y - cy) <= r,
        Region::Sector { cx, cy, inner, outer, start, end } => {
            let distance = (x - cx).hypot(y - cy);
            if distance < inner || distance > outer {
                return false;
            }
            let angle = start + ((y - cy).atan2(x - cx) - start).rem_euclid(TAU);
            angle <= end
        }
    }
}
