//! Drawing SVGs into pixels, for the clients that have nothing fast of their own to draw them with
//! and for the images marked up.

use std::sync::{Arc, OnceLock};

use resvg::usvg::{Options, Tree, fontdb};

/// The system's fonts, read once: the text in an SVG is set in them.
fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut fonts = fontdb::Database::new();
            fonts.load_system_fonts();
            // Where iOS and Android keep theirs, which `load_system_fonts` doesn't look in.
            fonts.load_fonts_dir("/System/Library/Fonts");
            fonts.load_fonts_dir("/system/fonts");
            Arc::new(fonts)
        })
        .clone()
}

/// The SVG read. Only one with text in it waits for the system's fonts, which take a while to read
/// the first time.
pub fn tree(data: &[u8]) -> Result<Tree, String> {
    let has_text = data.windows(5).any(|window| window == b"<text");
    let options = match has_text {
        true => Options { fontdb: fonts(), ..Options::default() },
        false => Options::default(),
    };
    Tree::from_data(data, &options).map_err(|e| e.to_string())
}

/// The SVG drawn `side` pixels on its longer side, as premultiplied RGBA, with its width and
/// height in pixels.
pub fn draw(data: &[u8], side: f32) -> Option<(u32, u32, Vec<u8>)> {
    let tree = tree(data).ok()?;
    let pixmap = draw_tree(&tree, side / tree.size().width().max(tree.size().height()))?;
    Some((pixmap.width(), pixmap.height(), pixmap.take()))
}

/// The SVG drawn at `scale` times its own size.
pub fn draw_tree(tree: &Tree, scale: f32) -> Option<tiny_skia::Pixmap> {
    let size = tree.size();
    let (width, height) =
        ((size.width() * scale).round().max(1.) as u32, (size.height() * scale).round().max(1.) as u32);
    let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_svg_is_drawn_at_the_size_asked_for_on_its_longer_side() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><rect width="12" height="12" fill="#ff0000"/></svg>"##;
        let (width, height, pixels) = draw(svg, 240.).unwrap();
        assert_eq!((width, height), (240, 120));
        assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
        let right = ((60 * width + 200) * 4) as usize;
        assert_eq!(pixels[right + 3], 0, "what the SVG leaves bare is see-through");
    }
}
