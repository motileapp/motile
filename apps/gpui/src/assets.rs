//! The app's own images, before the Lucide icons GPUI Kit bundles.

use std::borrow::Cow;

use gpui_kit::*;

const OWN: &[(&str, &[u8])] = &[
    ("icons/claude.svg", include_bytes!("../assets/icons/claude.svg")),
    ("icons/openai.svg", include_bytes!("../assets/icons/openai.svg")),
    ("icons/github-mark.svg", include_bytes!("../assets/icons/github-mark.svg")),
    ("icons/motile-mark.svg", include_bytes!("../assets/icons/motile-mark.svg")),
    ("icons/google-blue.svg", include_bytes!("../assets/icons/google-blue.svg")),
    ("icons/google-green.svg", include_bytes!("../assets/icons/google-green.svg")),
    ("icons/google-yellow.svg", include_bytes!("../assets/icons/google-yellow.svg")),
    ("icons/google-red.svg", include_bytes!("../assets/icons/google-red.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = OWN.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        // Lucide draws its strokes 2 units wide; SF Symbols, which the Mac app shows, are heavier.
        let loaded = gpui_kit::assets::AllAssets.load(path)?;
        Ok(loaded.map(|bytes| match std::str::from_utf8(&bytes) {
            Ok(svg) if svg.contains("stroke-width=\"2\"") => {
                Cow::Owned(svg.replace("stroke-width=\"2\"", "stroke-width=\"2.3\"").into_bytes())
            }
            _ => bytes,
        }))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names = gpui_kit::assets::AllAssets.list(path)?;
        names.extend(OWN.iter().filter(|(name, _)| name.starts_with(path)).map(|(name, _)| SharedString::from(*name)));
        Ok(names)
    }
}
