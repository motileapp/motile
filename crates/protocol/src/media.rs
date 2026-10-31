//! How the images, videos and other files a thread shows are named: by their contents, so a server
//! and a client that both have a file call it the same.

use std::borrow::Cow;
use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag};
use sha2::{Digest, Sha256};

pub const IMAGES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
pub const VIDEOS: [&str; 4] = ["mp4", "mov", "m4v", "webm"];
pub const MAX_SIZE: u64 = 200 * 1024 * 1024;
const HASH_LENGTH: usize = 64;
const MAX_EXTENSION: usize = 16;

/// The lowercase extension of a file that is shown as an image or a video, and whether it is a
/// video.
pub fn kind(path: &std::path::Path) -> Option<(String, bool)> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    let video = VIDEOS.contains(&extension.as_str());
    (video || IMAGES.contains(&extension.as_str())).then_some((extension, video))
}

/// The lowercase extension a copy of any other file is named with, empty for one without a plain
/// one.
pub fn extension(path: &std::path::Path) -> String {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    if is_extension(&extension) { extension } else { String::new() }
}

fn is_extension(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= MAX_EXTENSION
        && text.chars().all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
}

#[derive(Default)]
pub struct Namer(Sha256);

impl Namer {
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn id(self, extension: &str) -> String {
        let hash = format!("{:x}", self.0.finalize());
        if extension.is_empty() { hash } else { format!("{hash}.{extension}") }
    }
}

/// Ids come back from clients, so they are held to the shape they are made in.
pub fn is_id(id: &str) -> bool {
    let (hash, extension) = id.split_once('.').map_or((id, None), |(hash, extension)| (hash, Some(extension)));
    hash.len() == HASH_LENGTH
        && hash.chars().all(|character| character.is_ascii_hexdigit())
        && extension.is_none_or(is_extension)
}

/// `text` with the path of each image that has spaces in angle brackets, as Markdown asks of one
/// and agents often leave out: `![Report](/a b.pdf)` reads as `![Report](</a b.pdf>)`.
pub fn bracket_spaced_images(text: &str) -> Cow<'_, str> {
    let mut code: Option<Vec<Range<usize>>> = None;
    let (mut bracketed, mut copied, mut from) = (String::new(), 0, 0);
    while let Some(found) = text[from..].find("![") {
        let alt = from + found + 2;
        from = alt;
        let Some((path, end)) = spaced_path(text, alt) else { continue };
        if code.get_or_insert_with(|| code_ranges(text)).iter().any(|range| range.contains(&alt)) {
            continue;
        }
        bracketed.push_str(&text[copied..path.start]);
        bracketed.push('<');
        bracketed.push_str(&text[path.clone()]);
        bracketed.push('>');
        (copied, from) = (path.end, end);
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    bracketed.push_str(&text[copied..]);
    Cow::Owned(bracketed)
}

/// Where the path of the image whose alt text starts at `alt` is, when it has spaces and no angle
/// brackets, and where the image ends.
fn spaced_path(text: &str, alt: usize) -> Option<(Range<usize>, usize)> {
    let line = &text[alt..text[alt..].find('\n').map_or(text.len(), |end| alt + end)];
    let alt_end = closing(line, '[', ']')?;
    let inside = line[alt_end + 1..].strip_prefix('(')?;
    let inside = &inside[..closing(inside, '(', ')')?];
    let path = match inside.strip_suffix('"').and_then(|quoted| quoted.rfind(" \"")) {
        Some(title) => &inside[..title],
        None => inside,
    };
    let leading = path.len() - path.trim_start().len();
    let path = path.trim();
    if !path.contains(' ') || path.contains(['<', '>']) {
        return None;
    }
    let start = alt + alt_end + 2 + leading;
    Some((start..start + path.len(), alt + alt_end + 2 + inside.len() + 1))
}

/// Where in `text` the `close` that ends what `open`s nest in is, after a backslash's escape.
fn closing(text: &str, open: char, close: char) -> Option<usize> {
    let (mut depth, mut escaped) = (0, false);
    for (index, character) in text.char_indices() {
        match character {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            _ if character == open => depth += 1,
            _ if character == close && depth == 0 => return Some(index),
            _ if character == close => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The code and HTML of `text`, where an image is only text.
fn code_ranges(text: &str) -> Vec<Range<usize>> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let ranges = Parser::new_ext(text, options).into_offset_iter().filter_map(|(event, range)| match event {
        Event::Code(_) | Event::InlineHtml(_) | Event::Start(Tag::CodeBlock(_) | Tag::HtmlBlock) => Some(range),
        _ => None,
    });
    ranges.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_names_made_here_are_ids() {
        let mut namer = Namer::default();
        namer.update(b"picture");
        let id = namer.id("png");
        assert!(is_id(&id));
        let hash = "a".repeat(HASH_LENGTH);
        assert!(is_id(&format!("{hash}.pdf")) && is_id(&hash));
        assert!(!is_id(&format!("{hash}.tar.gz")));
        assert!(!is_id(&format!("{hash}.PDF")));
        assert!(!is_id(&format!("{hash}.")));
        assert!(!is_id("../motile.sqlite"));
        assert!(!is_id(&format!("../{hash}.png")));
        assert!(!is_id(&format!("{hash}/../motile.sqlite")));
    }

    #[test]
    fn only_images_and_videos_have_a_kind() {
        assert_eq!(kind(std::path::Path::new("/a/Shot.PNG")), Some(("png".to_string(), false)));
        assert_eq!(kind(std::path::Path::new("demo.mov")), Some(("mov".to_string(), true)));
        assert_eq!(kind(std::path::Path::new("notes.txt")), None);
    }

    #[test]
    fn any_other_file_keeps_a_plain_extension() {
        assert_eq!(extension(std::path::Path::new("/a/Report.PDF")), "pdf");
        assert_eq!(extension(std::path::Path::new("logs.tar.gz")), "gz");
        assert_eq!(extension(std::path::Path::new("Makefile")), "");
        assert_eq!(extension(std::path::Path::new("odd.ext-ension")), "");
    }

    #[test]
    fn an_image_path_with_spaces_is_bracketed() {
        let cases = [
            ("![Book](/out/A b (Dort) - C.epub)", "![Book](</out/A b (Dort) - C.epub>)"),
            (
                "![Demo](my demo.mp4 \"my poster.png\") and ![x](y z.png)",
                "![Demo](<my demo.mp4> \"my poster.png\") and ![x](<y z.png>)",
            ),
            ("![a [b] c]( a b.png )", "![a [b] c]( <a b.png> )"),
            ("![a](a.png) ![b](<b c.png>) ![c](c d.png", "![a](a.png) ![b](<b c.png>) ![c](c d.png"),
            ("`![a](a b.png)`\n\n```\n![a](a b.png)\n```", "`![a](a b.png)`\n\n```\n![a](a b.png)\n```"),
        ];
        for (text, read) in cases {
            assert_eq!(bracket_spaced_images(text), read);
        }
    }
}
