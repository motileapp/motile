//! How the images, videos and other files a thread shows are named: by their contents, so a server
//! and a client that both have a file call it the same.

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
}
