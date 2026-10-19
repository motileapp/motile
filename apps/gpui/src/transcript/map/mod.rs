//! OpenUI's Map, drawn in Rust: OpenFreeMap's vector tiles in the style `packages/theme` writes,
//! the pins and the route over them, with the camera core moves (`motile_core::render::openui::map`).
//! The same on Linux, Windows and the Mac, with no web view and no MapLibre.

#![allow(dead_code, reason = "the transcript's Ui rows embed the map in a later step")]

pub mod draw;
pub mod paint;
pub mod pool;
#[cfg(test)]
mod raster;
pub mod style;
pub mod tiles;
pub mod view;

/// The height of a map in a window this wide: the node's (core clamps it 200–720), at most 0.82
/// of the window's width, and never under 280. The measuring pass asks it; it creates no map.
pub fn height(node_height: f32, window_width: f32) -> f32 {
    node_height.min((0.82 * window_width).max(280.))
}

#[cfg(test)]
mod tests {
    use super::paint::{self, tests::*};
    use super::style::{color, style};

    /// Draws the fixture's map in both appearances, into `MOTILE_MAP_PICTURES` or the temporary
    /// folder: `gpui-map-dark.png` and `gpui-map-light.png`.
    #[test]
    fn the_fixture_map_makes_a_picture() {
        let folder = std::env::var_os("MOTILE_MAP_PICTURES").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
        let view = fixture_view();
        for (dark, name, water) in [(true, "dark", "#1d3540"), (false, "light", "#a2dff5")] {
            let list = paint::paint(style(dark), &placed(&view), &view, &fixture_pins(), &fixture_line());
            let water = color(&water.into()).unwrap();
            let picture = super::raster::picture(&list, view.width as f32, view.height as f32, 2., water, 16.);
            let pixel = picture.pixel(400, 400).unwrap();
            assert_eq!(pixel.alpha(), 255);
            picture.save_png(folder.join(format!("gpui-map-{name}.png"))).unwrap();
        }
    }

    #[test]
    fn a_narrow_window_makes_the_map_shorter() {
        assert_eq!(super::height(400., 1200.), 400.);
        assert_eq!(super::height(400., 390.), 319.8);
        assert_eq!(super::height(400., 300.), 280.);
        assert_eq!(super::height(240., 300.), 240.);
    }
}
