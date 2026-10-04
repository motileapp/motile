//! How the images and videos a thread shows are named: by their contents, so a server and a client
//! that both have a file call it the same.

use sha2::{Digest, Sha256};

pub const IMAGES: [&str; 5] = ["png", "jpg", "jpeg", "gif", "webp"];
pub const VIDEOS: [&str; 4] = ["mp4", "mov", "m4v", "webm"];
pub const MAX_SIZE: u64 = 200 * 1024 * 1024;
const HASH_LENGTH: usize = 64;

/// The lowercase extension of a file that is shown as an image or a video, and whether it is a
/// video.
pub fn kind(path: &std::path::Path) -> Option<(String, bool)> {
    let extension = path.extension()?.to_str()?.to_lowercase();
    let video = VIDEOS.contains(&extension.as_str());
    (video || IMAGES.contains(&extension.as_str())).then_some((extension, video))
}

#[derive(Default)]
pub struct Namer(Sha256);

impl Namer {
    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    pub fn id(self, extension: &str) -> String {
        format!("{:x}.{extension}", self.0.finalize())
    }
}

/// Ids come back from clients, so they are held to the shape they are made in.
pub fn is_id(id: &str) -> bool {
    let Some((hash, extension)) = id.split_once('.') else { return false };
    hash.len() == HASH_LENGTH
        && hash.chars().all(|character| character.is_ascii_hexdigit())
        && (IMAGES.contains(&extension) || VIDEOS.contains(&extension))
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
        assert!(!is_id(&format!("{hash}.sqlite")));
        assert!(!is_id("../motile.sqlite"));
        assert!(!is_id(&format!("../{hash}.png")));
    }

    #[test]
    fn only_images_and_videos_have_a_kind() {
        assert_eq!(kind(std::path::Path::new("/a/Shot.PNG")), Some(("png".to_string(), false)));
        assert_eq!(kind(std::path::Path::new("demo.mov")), Some(("mov".to_string(), true)));
        assert_eq!(kind(std::path::Path::new("notes.txt")), None);
    }
}
