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
    ("icons/smile-plus.svg", include_bytes!("../assets/icons/smile-plus.svg")),
    ("icons/linear.svg", include_bytes!("../assets/icons/linear.svg")),
    ("icons/effort-low.svg", include_bytes!("../assets/icons/effort-low.svg")),
    ("icons/effort-medium.svg", include_bytes!("../assets/icons/effort-medium.svg")),
    ("icons/effort-high.svg", include_bytes!("../assets/icons/effort-high.svg")),
    ("icons/effort-extra.svg", include_bytes!("../assets/icons/effort-extra.svg")),
    ("icons/chevron-down-trimmed.svg", include_bytes!("../assets/icons/chevron-down-trimmed.svg")),
    ("icons/chevron-right-trimmed.svg", include_bytes!("../assets/icons/chevron-right-trimmed.svg")),
    ("icons/undo-2-trimmed.svg", include_bytes!("../assets/icons/undo-2-trimmed.svg")),
];

/// DM Sans, the app's typeface, in the weights the views use.
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/DMSans-Regular.ttf"),
    include_bytes!("../assets/fonts/DMSans-Medium.ttf"),
    include_bytes!("../assets/fonts/DMSans-SemiBold.ttf"),
    include_bytes!("../assets/fonts/DMSans-Bold.ttf"),
    include_bytes!("../assets/fonts/DMSans-Italic.ttf"),
    include_bytes!("../assets/fonts/DMSans-MediumItalic.ttf"),
    include_bytes!("../assets/fonts/DMSans-SemiBoldItalic.ttf"),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = OWN.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut names = gpui_kit::assets::AllAssets.list(path)?;
        names.extend(OWN.iter().filter(|(name, _)| name.starts_with(path)).map(|(name, _)| SharedString::from(*name)));
        Ok(names)
    }
}
