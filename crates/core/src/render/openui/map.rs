//! The camera of OpenUI's map, as open-intelligent-ui's `map.tsx` moves MapLibre's: where the first
//! pin puts it, how every pin is fitted, the fly between two cameras and the route's reveal. Zoom
//! levels are MapLibre's, on 512-point tiles, so every client's engine takes them as they are.

use serde::{Deserialize, Serialize};

pub const TILE: f64 = 512.0;
pub const FIRST_ZOOM: f64 = 10.5;
pub const FLY_MS: u32 = 920;
pub const DRAW_MS: u32 = 760;
pub const MIN_ZOOM: f64 = 0.0;
pub const MAX_ZOOM: f64 = 20.0;
const MAX_LATITUDE: f64 = 85.051_128_779_806_59;
const RHO: f64 = 1.42;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct Camera {
    pub lat: f64,
    pub lng: f64,
    pub zoom: f64,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Framing {
    /// The first pin while the program streams: a jump to a city's overview.
    First,
    /// Every pin shown once the program ends, or the filter changes.
    Fit,
    /// Every pin shown, from the recenter button.
    Recenter,
}

struct Insets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}

/// Where the camera goes for these pins (`[lat, lng]`) in a map this many points wide and tall.
pub fn frame(points: &[[f64; 2]], width: f64, height: f64, framing: Framing) -> Option<Camera> {
    let first = points.first()?;
    let (padding, max_zoom) = match framing {
        Framing::First => return Some(Camera { lat: first[0], lng: first[1], zoom: FIRST_ZOOM }),
        Framing::Fit => (Insets { top: 70.0, left: 72.0, bottom: 50.0, right: 72.0 }, 13.0),
        Framing::Recenter => (Insets { top: 55.0, left: 34.0, bottom: 35.0, right: 40.0 }, 14.0),
    };
    let world: Vec<[f64; 2]> = points.iter().map(|point| world(point[0], point[1])).collect();
    let (min, max) = world.iter().fold(([f64::MAX; 2], [f64::MIN; 2]), |(min, max), point| {
        ([min[0].min(point[0]), min[1].min(point[1])], [max[0].max(point[0]), max[1].max(point[1])])
    });
    let inner = |size: f64, a: f64, b: f64| if size - a - b > 0.0 { size - a - b } else { size };
    let (room_x, room_y) = (inner(width, padding.left, padding.right), inner(height, padding.top, padding.bottom));
    let scale_x = if max[0] > min[0] { room_x / (max[0] - min[0]) } else { f64::INFINITY };
    let scale_y = if max[1] > min[1] { room_y / (max[1] - min[1]) } else { f64::INFINITY };
    let zoom = scale_x.min(scale_y).log2().clamp(MIN_ZOOM, max_zoom);
    let scale = zoom.exp2();
    let center = [
        (min[0] + max[0]) / 2.0 - (padding.left - padding.right) / 2.0 / scale,
        (min[1] + max[1]) / 2.0 - (padding.top - padding.bottom) / 2.0 / scale,
    ];
    let (lat, lng) = unworld(center);
    Some(Camera { lat, lng, zoom })
}

/// The camera `t` (0–1, not eased) of the way along MapLibre's fly from `from` to `to`, for a map
/// this many points wide and tall: it zooms out and back in on the way when they are far apart.
pub fn fly(from: Camera, to: Camera, width: f64, height: f64, t: f64) -> Camera {
    let t = cubic_bezier(0.25, 0.1, 0.25, 1.0, t.clamp(0.0, 1.0));
    if t >= 1.0 {
        return to;
    }
    let (start, end) = (world(from.lat, from.lng), world(to.lat, to.lng));
    let w0 = width.max(height);
    let w1 = w0 / (to.zoom - from.zoom).exp2();
    let u1 = (end[0] - start[0]).hypot(end[1] - start[1]) * from.zoom.exp2();
    let rho2 = RHO * RHO;
    let r = |end: bool| {
        let sign = if end { -1.0 } else { 1.0 };
        let b = (w1 * w1 - w0 * w0 + sign * rho2 * rho2 * u1 * u1) / (2.0 * if end { w1 } else { w0 } * rho2 * u1);
        ((b * b + 1.0).sqrt() - b).ln()
    };
    let r0 = r(false);
    let length = (r(true) - r0) / RHO;
    let (along, scale) = if u1 < 1e-6 || !length.is_finite() {
        let k = if w1 < w0 { -1.0 } else { 1.0 };
        let s = t * (w1 / w0).ln().abs() / RHO;
        (0.0, 1.0 / (k * RHO * s).exp())
    } else {
        let s = t * length;
        let along = w0 * ((r0.cosh() * (r0 + RHO * s).tanh() - r0.sinh()) / rho2) / u1;
        (along, (r0 + RHO * s).cosh() / r0.cosh())
    };
    let (lat, lng) = unworld([start[0] + (end[0] - start[0]) * along, start[1] + (end[1] - start[1]) * along]);
    Camera { lat, lng, zoom: (from.zoom + scale.log2()).clamp(MIN_ZOOM, MAX_ZOOM) }
}

/// The route as drawn `t` (0–1, not eased) of the way through its reveal: its first points, cut
/// at that share of its length, eased in and out.
pub fn reveal(line: &[[f64; 2]], t: f64) -> Vec<[f64; 2]> {
    let Some(first) = line.first() else { return Vec::new() };
    let t = t.clamp(0.0, 1.0);
    let progress = if t < 0.5 { 2.0 * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(2) / 2.0 };
    let lengths: Vec<f64> =
        line.windows(2).map(|pair| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1])).collect();
    let mut remaining = lengths.iter().sum::<f64>() * progress;
    let mut out = vec![*first];
    for (index, length) in lengths.iter().enumerate() {
        let (a, b) = (line[index], line[index + 1]);
        if remaining >= *length {
            out.push(b);
            remaining -= length;
            continue;
        }
        let share = if *length > 0.0 { remaining / length } else { 0.0 };
        out.push([a[0] + (b[0] - a[0]) * share, a[1] + (b[1] - a[1]) * share]);
        break;
    }
    out
}

/// A point's place on the world at zoom 0, in points from its top left corner (Web Mercator).
pub fn world(lat: f64, lng: f64) -> [f64; 2] {
    let lat = lat.clamp(-MAX_LATITUDE, MAX_LATITUDE).to_radians();
    let x = (lng + 180.0) / 360.0;
    let y = 0.5 - (lat.tan() + 1.0 / lat.cos()).ln() / (2.0 * std::f64::consts::PI);
    [x * TILE, y * TILE]
}

/// `[lat, lng]` of a place on the world at zoom 0.
pub fn unworld(point: [f64; 2]) -> (f64, f64) {
    let lng = point[0] / TILE * 360.0 - 180.0;
    let n = std::f64::consts::PI * (1.0 - 2.0 * point[1] / TILE);
    (n.sinh().atan().to_degrees(), lng)
}

/// Where a place lies in a map this many points wide and tall, from its top left corner.
pub fn project(camera: Camera, width: f64, height: f64, lat: f64, lng: f64) -> [f64; 2] {
    let scale = camera.zoom.exp2();
    let center = world(camera.lat, camera.lng);
    let point = world(lat, lng);
    [(point[0] - center[0]) * scale + width / 2.0, (point[1] - center[1]) * scale + height / 2.0]
}

/// The camera after the map is dragged by `dx`, `dy` points.
pub fn pan(camera: Camera, dx: f64, dy: f64) -> Camera {
    let scale = camera.zoom.exp2();
    let center = world(camera.lat, camera.lng);
    let (lat, lng) = unworld([center[0] - dx / scale, (center[1] - dy / scale).clamp(0.0, TILE)]);
    Camera { lat, lng: wrap(lng), zoom: camera.zoom }
}

/// The camera zoomed by `by` levels about a point of the map, which stays where it is.
pub fn zoom_about(camera: Camera, width: f64, height: f64, by: f64, about: [f64; 2]) -> Camera {
    let zoom = (camera.zoom + by).clamp(MIN_ZOOM, MAX_ZOOM);
    let ratio = 1.0 - (camera.zoom - zoom).exp2();
    let (dx, dy) = ((about[0] - width / 2.0) * ratio, (about[1] - height / 2.0) * ratio);
    Camera { zoom, ..pan(camera, -dx, -dy) }
}

fn wrap(lng: f64) -> f64 {
    (lng + 180.0).rem_euclid(360.0) - 180.0
}

/// CSS's `cubic-bezier`, which MapLibre's fly is timed by (its default `ease`).
fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, t: f64) -> f64 {
    let curve = |a: f64, b: f64, s: f64| 3.0 * a * s * (1.0 - s).powi(2) + 3.0 * b * s * s * (1.0 - s) + s.powi(3);
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..40 {
        let middle = (low + high) / 2.0;
        if curve(x1, x2, middle) < t {
            low = middle;
        } else {
            high = middle;
        }
    }
    curve(y1, y2, (low + high) / 2.0)
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Ask {
    Frame { points: Vec<[f64; 2]>, width: f64, height: f64, framing: Framing },
    Fly { from: Camera, to: Camera, width: f64, height: f64, t: f64 },
    Reveal { line: Vec<[f64; 2]>, t: f64 },
}

/// The camera maths for the clients whose engines are MapLibre's: `{"op": "frame", "points",
/// "width", "height", "framing"}` answers a camera or null, `{"op": "fly", "from", "to", "width",
/// "height", "t"}` a camera, `{"op": "reveal", "line", "t"}` the line to draw.
pub fn map_json(input: &str) -> String {
    let answer = match serde_json::from_str::<Ask>(input) {
        Ok(Ask::Frame { points, width, height, framing }) => {
            serde_json::to_value(frame(&points, width, height, framing))
        }
        Ok(Ask::Fly { from, to, width, height, t }) => serde_json::to_value(fly(from, to, width, height, t)),
        Ok(Ask::Reveal { line, t }) => serde_json::to_value(reveal(&line, t)),
        Err(error) => {
            tracing::warn!("couldn't read a map question: {error}");
            Ok(serde_json::Value::Null)
        }
    };
    answer.map(|value| value.to_string()).unwrap_or_else(|_| "null".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, within: f64) -> bool {
        (a - b).abs() < within
    }

    #[test]
    fn the_first_pin_is_a_city_overview() {
        let camera = frame(&[[48.85, 2.35], [48.86, 2.29]], 720.0, 400.0, Framing::First).unwrap();
        assert_eq!(camera, Camera { lat: 48.85, lng: 2.35, zoom: 10.5 });
        assert_eq!(frame(&[], 720.0, 400.0, Framing::Fit), None);
    }

    #[test]
    fn one_pin_fits_at_the_most_zoom() {
        let camera = frame(&[[48.85, 2.35]], 720.0, 400.0, Framing::Fit).unwrap();
        assert_eq!(camera.zoom, 13.0);
        let recentered = frame(&[[48.85, 2.35]], 720.0, 400.0, Framing::Recenter).unwrap();
        assert_eq!(recentered.zoom, 14.0);
    }

    #[test]
    fn every_pin_lands_inside_the_padding() {
        let points = [[48.8584, 2.2945], [48.8606, 2.3376], [48.8530, 2.3499], [48.8867, 2.3431]];
        let (width, height) = (720.0, 400.0);
        let camera = frame(&points, width, height, Framing::Fit).unwrap();
        let projected: Vec<[f64; 2]> = points.iter().map(|p| project(camera, width, height, p[0], p[1])).collect();
        let left = projected.iter().map(|p| p[0]).fold(f64::MAX, f64::min);
        let right = projected.iter().map(|p| p[0]).fold(f64::MIN, f64::max);
        let top = projected.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
        let bottom = projected.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
        assert!(left >= 72.0 - 0.01 && right <= width - 72.0 + 0.01, "{left} {right}");
        assert!(top >= 70.0 - 0.01 && bottom <= height - 50.0 + 0.01, "{top} {bottom}");
        assert!(close(top, 70.0, 0.01) || close(left, 72.0, 0.01), "the pins touch one pair of edges");
        assert!(close(camera.zoom, 11.908, 0.001), "{}", camera.zoom);
    }

    #[test]
    fn the_world_round_trips() {
        let (lat, lng) = unworld(world(-33.8688, 151.2093));
        assert!(close(lat, -33.8688, 1e-9) && close(lng, 151.2093, 1e-9));
        let camera = Camera { lat: 51.5, lng: -0.12, zoom: 11.0 };
        assert_eq!(project(camera, 400.0, 300.0, 51.5, -0.12), [200.0, 150.0]);
    }

    #[test]
    fn a_drag_moves_the_place_under_the_pointer_along() {
        let camera = Camera { lat: 51.5, lng: -0.12, zoom: 11.0 };
        let moved = pan(camera, 30.0, -20.0);
        let point = project(moved, 400.0, 300.0, 51.5, -0.12);
        assert!(close(point[0], 230.0, 1e-6) && close(point[1], 130.0, 1e-6), "{point:?}");
    }

    #[test]
    fn zooming_keeps_the_point_it_is_about() {
        let camera = Camera { lat: 51.5, lng: -0.12, zoom: 11.0 };
        let about = [300.0, 100.0];
        let before = pan(camera, 0.0, 0.0);
        let zoomed = zoom_about(camera, 400.0, 300.0, 1.0, about);
        assert_eq!(zoomed.zoom, 12.0);
        let place = {
            let scale = before.zoom.exp2();
            let center = world(before.lat, before.lng);
            unworld([center[0] + (about[0] - 200.0) / scale, center[1] + (about[1] - 150.0) / scale])
        };
        let point = project(zoomed, 400.0, 300.0, place.0, place.1);
        assert!(close(point[0], 300.0, 1e-6) && close(point[1], 100.0, 1e-6), "{point:?}");
    }

    #[test]
    fn a_fly_starts_and_ends_on_its_cameras_and_zooms_out_between() {
        let from = Camera { lat: 48.85, lng: 2.35, zoom: 10.5 };
        let to = Camera { lat: 51.5, lng: -0.12, zoom: 11.0 };
        let start = fly(from, to, 720.0, 400.0, 0.0);
        let end = fly(from, to, 720.0, 400.0, 1.0);
        assert!(close(start.lat, from.lat, 1e-6) && close(start.zoom, from.zoom, 1e-6), "{start:?}");
        assert!(
            close(end.lat, to.lat, 1e-6) && close(end.lng, to.lng, 1e-6) && close(end.zoom, to.zoom, 1e-6),
            "{end:?}"
        );
        assert!(fly(from, to, 720.0, 400.0, 0.5).zoom < 10.0);
        let still = fly(from, from, 720.0, 400.0, 0.5);
        assert!(close(still.lat, from.lat, 1e-9) && close(still.zoom, from.zoom, 1e-9));
    }

    #[test]
    fn the_route_is_revealed_along_its_length() {
        let line = [[0.0, 0.0], [0.0, 1.0], [0.0, 3.0]];
        assert_eq!(reveal(&line, 0.0), vec![[0.0, 0.0], [0.0, 0.0]]);
        assert_eq!(reveal(&line, 0.5), vec![[0.0, 0.0], [0.0, 1.0], [0.0, 1.5]]);
        assert_eq!(reveal(&line, 1.0), line.to_vec());
        assert_eq!(reveal(&line, 0.25), vec![[0.0, 0.0], [0.0, 0.375]]);
        assert!(reveal(&[], 0.5).is_empty());
    }

    #[test]
    fn the_clients_ask_in_json() {
        let camera = map_json(r#"{"op":"frame","points":[[1,2]],"width":300,"height":200,"framing":"first"}"#);
        assert_eq!(camera, r#"{"lat":1.0,"lng":2.0,"zoom":10.5}"#);
        assert_eq!(map_json(r#"{"op":"reveal","line":[[0,0],[0,2]],"t":1}"#), "[[0.0,0.0],[0.0,2.0]]");
        assert_eq!(map_json("nonsense"), "null");
    }
}
