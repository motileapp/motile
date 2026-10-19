//! The transcript of the open thread: the rows the core sends, and the list that draws them.

pub mod kit;
pub mod map;
pub mod model;
pub mod prose;
pub mod rows;
pub mod view;

use gpui_kit::*;

/// Lays out a little of every kind of text the transcript has once, at launch, so that the first
/// reply doesn't pay for loading the fonts while it streams, as the Mac's `RowTextView.warmUp`.
pub fn warm_up(window: &mut Window) {
    let samples = [
        rows::font(crate::theme::UI_FONT, FontWeight::NORMAL, false),
        rows::font(crate::theme::UI_FONT, FontWeight::SEMIBOLD, false),
        rows::font(crate::theme::UI_FONT, FontWeight::NORMAL, true),
        rows::font(crate::theme::MONO_FONT, FontWeight::NORMAL, false),
        crate::fonts::inline_code(crate::theme::PROSE_SIZE, false),
    ];
    for font in samples {
        let text = SharedString::from("Warm up");
        let run = TextRun {
            len: text.len(),
            font,
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window.text_system().shape_line(text, px(crate::theme::PROSE_SIZE), &[run], None);
    }
}
