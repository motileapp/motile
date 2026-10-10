//! The images and videos agents show in their replies and users attach to their messages, and the
//! other files agents send. The server copies each one when it is shown, named by its contents, so
//! a thread keeps showing it after the file has changed or gone.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use motile_protocol::media::{MAX_SIZE, Namer, extension, is_id, kind};
use motile_protocol::wire::Media;
use pulldown_cmark::{Event, Options, Parser, Tag};

/// The image a client made to stand for a video it attached, in the video's folder.
pub const POSTER: &str = ".poster.jpg";

#[derive(Clone)]
pub struct MediaStore {
    folder: PathBuf,
    home: String,
}

impl MediaStore {
    pub fn new(folder: PathBuf, home: &str) -> Self {
        Self { folder, home: home.to_string() }
    }

    /// Copies what `text` shows or sends and `known` doesn't hold yet, each video with the image
    /// its title names as its poster. Paths are taken from `cwd`.
    pub fn capture_new(&self, text: &str, cwd: &str, known: &[Media]) -> Vec<Media> {
        if !text.contains("![") {
            return Vec::new();
        }
        let mut captured: Vec<Media> = Vec::new();
        for (src, title) in images(text) {
            if known.iter().chain(&captured).any(|media| media.src == src) {
                continue;
            }
            let Some(mut media) = self.capture(&src, cwd, true) else { continue };
            if media.video && !title.is_empty() {
                self.give_poster(&mut media, &title, cwd);
            }
            captured.push(media);
        }
        captured
    }

    /// Copies the images and videos among the files attached to a message, each video with the
    /// poster that was uploaded for it.
    pub fn capture_attached(&self, attachments: &[String]) -> Vec<Media> {
        let mut captured: Vec<Media> = Vec::new();
        for path in attachments {
            if captured.iter().any(|media| &media.src == path) {
                continue;
            }
            let Some(mut media) = self.capture(path, "/", false) else { continue };
            if let Some(poster) = Path::new(path).with_file_name(POSTER).to_str().filter(|_| media.video) {
                self.give_poster(&mut media, poster, "/");
            }
            captured.push(media);
        }
        captured
    }

    /// Has the image at `poster` stand for the video until it plays, and give it its size.
    fn give_poster(&self, video: &mut Media, poster: &str, cwd: &str) {
        let Some(poster) = self.capture(poster, cwd, false).filter(|poster| !poster.video) else { return };
        (video.width, video.height) = (poster.width, poster.height);
        video.poster = Some(poster.id);
    }

    /// Copies the image or the video at `src`, or with `any_file` whatever file is there.
    fn capture(&self, src: &str, cwd: &str, any_file: bool) -> Option<Media> {
        let path = self.file_shown(src, cwd)?;
        let shown = kind(&path);
        if shown.is_none() && !any_file {
            return None;
        }
        let size = std::fs::metadata(&path).ok().filter(|file| file.is_file())?.len();
        if size == 0 || size > MAX_SIZE {
            return None;
        }
        let (extension, video) = shown.clone().unwrap_or_else(|| (extension(&path), false));
        let (id, size) = match self.copy(&path, &extension) {
            Ok(copied) => copied,
            Err(error) => {
                tracing::warn!("couldn't keep {}: {error}", path.display());
                return None;
            }
        };
        let media = Media {
            id,
            src: src.to_string(),
            video,
            file: shown.is_none(),
            size,
            width: None,
            height: None,
            poster: None,
        };
        if video || media.file {
            return Some(media);
        }
        // A file that only has an image's name isn't shown as one.
        let Ok(dimensions) = imagesize::size(self.folder.join(&media.id)) else {
            if any_file {
                return Some(Media { file: true, ..media });
            }
            self.remove(&media.id);
            return None;
        };
        let (width, height) = (u32::try_from(dimensions.width).ok(), u32::try_from(dimensions.height).ok());
        Some(Media { width, height, ..media })
    }

    /// The file a reply's image points at, if it is one on this machine.
    fn file_shown(&self, src: &str, cwd: &str) -> Option<PathBuf> {
        let src = src.strip_prefix("file://").unwrap_or(src);
        if src.contains("://") || src.starts_with("data:") {
            return None;
        }
        let located = |text: &str| {
            let path = match text.strip_prefix("~/") {
                Some(rest) => Path::new(&self.home).join(rest),
                None => Path::new(cwd).join(text),
            };
            path.canonicalize().ok()
        };
        located(src).or_else(|| located(&percent_decode(src)))
    }

    /// Copies the file into the store and returns the name its contents give it, and its size.
    fn copy(&self, path: &Path, extension: &str) -> std::io::Result<(String, u64)> {
        std::fs::create_dir_all(&self.folder)?;
        let unfinished = self.folder.join(format!("{}.part", uuid::Uuid::new_v4().simple()));
        let copied = (|| {
            let mut source = std::fs::File::open(path)?;
            let mut copy = std::fs::File::create(&unfinished)?;
            let mut namer = Namer::default();
            let mut buffer = vec![0u8; 64 * 1024];
            let mut size = 0u64;
            loop {
                let read = source.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                namer.update(&buffer[..read]);
                copy.write_all(&buffer[..read])?;
                size += read as u64;
            }
            let id = namer.id(extension);
            std::fs::rename(&unfinished, self.folder.join(&id))?;
            Ok((id, size))
        })();
        if copied.is_err() {
            let _ = std::fs::remove_file(&unfinished);
        }
        copied
    }

    pub async fn open(&self, id: &str) -> anyhow::Result<(tokio::fs::File, u64)> {
        if !is_id(id) {
            bail!("{id} isn't the name of a file your server keeps.");
        }
        let file = tokio::fs::File::open(self.folder.join(id)).await.context("Your server no longer has that file.")?;
        let size = file.metadata().await?.len();
        Ok((file, size))
    }

    pub fn remove(&self, id: &str) {
        if is_id(id) {
            let _ = std::fs::remove_file(self.folder.join(id));
        }
    }
}

/// Where the images of a reply point, as the clients' Markdown parser reads them, with their titles.
fn images(text: &str) -> Vec<(String, String)> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let images = Parser::new_ext(text, options).filter_map(|event| match event {
        Event::Start(Tag::Image { dest_url, title, .. }) => Some((dest_url.to_string(), title.to_string())),
        _ => None,
    });
    images.collect()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let hex = (bytes[index] == b'%').then(|| text.get(index + 1..index + 3)).flatten();
        match hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first bytes of a PNG that is `width` by `height`; enough to read its size from.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        bytes
    }

    fn store(dir: &tempfile::TempDir) -> (MediaStore, String) {
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        let home = dir.path().to_string_lossy().into_owned();
        (MediaStore::new(dir.path().join("media"), &home), project.to_string_lossy().into_owned())
    }

    #[test]
    fn images_are_found_by_relative_absolute_and_home_paths() {
        let dir = tempfile::tempdir().unwrap();
        let (store, cwd) = store(&dir);
        std::fs::write(dir.path().join("project/shot one.png"), png(640, 400)).unwrap();
        std::fs::write(dir.path().join("wide.png"), png(1200, 300)).unwrap();
        let absolute = dir.path().join("wide.png");
        let text = format!(
            "Before\n\n![One](shot%20one.png)\n\n![Two]({})\n\n![Three](~/wide.png)\n\n`![not](this.png)`",
            absolute.display()
        );

        let captured = store.capture_new(&text, &cwd, &[]);

        let shown: Vec<(&str, Option<u32>, Option<u32>)> =
            captured.iter().map(|media| (media.src.as_str(), media.width, media.height)).collect();
        assert_eq!(
            shown,
            vec![
                ("shot%20one.png", Some(640), Some(400)),
                (absolute.to_str().unwrap(), Some(1200), Some(300)),
                ("~/wide.png", Some(1200), Some(300)),
            ]
        );
        assert_eq!(captured[1].id, captured[2].id, "the same contents are kept once");
        assert!(store.capture_new(&text, &cwd, &captured).is_empty(), "what is held isn't copied again");
    }

    #[test]
    fn the_copy_outlives_the_file_and_any_other_file_is_sent() {
        let dir = tempfile::tempdir().unwrap();
        let (store, cwd) = store(&dir);
        let shot = dir.path().join("project/shot.png");
        std::fs::write(&shot, png(10, 20)).unwrap();
        std::fs::write(dir.path().join("project/demo.mp4"), b"not really a video").unwrap();
        std::fs::write(dir.path().join("project/notes.png"), b"not an image").unwrap();
        std::fs::write(dir.path().join("project/report.pdf"), b"a report").unwrap();
        std::fs::create_dir_all(dir.path().join("project/folder")).unwrap();
        let text = "![a](shot.png) ![b](demo.mp4) ![c](notes.png) ![d](report.pdf) ![e](folder) ![f](https://example.com/a.png)";

        let captured = store.capture_new(text, &cwd, &[]);
        std::fs::remove_file(&shot).unwrap();

        let kept: Vec<(&str, bool, bool)> =
            captured.iter().map(|media| (media.src.as_str(), media.video, media.file)).collect();
        assert_eq!(
            kept,
            vec![
                ("shot.png", false, false),
                ("demo.mp4", true, false),
                ("notes.png", false, true),
                ("report.pdf", false, true)
            ]
        );
        assert!(captured[3].id.ends_with(".pdf"));
        assert_eq!(std::fs::read(dir.path().join("media").join(&captured[0].id)).unwrap(), png(10, 20));
        assert_eq!(std::fs::read(dir.path().join("media").join(&captured[3].id)).unwrap(), b"a report");
        assert_eq!(std::fs::read_dir(dir.path().join("media")).unwrap().count(), 4);
    }

    #[test]
    fn a_video_has_the_image_its_title_names_as_its_poster() {
        let dir = tempfile::tempdir().unwrap();
        let (store, cwd) = store(&dir);
        std::fs::write(dir.path().join("project/demo.mp4"), b"a video").unwrap();
        std::fs::write(dir.path().join("project/demo.png"), png(1280, 720)).unwrap();
        let text = "![Demo](demo.mp4 \"demo.png\") ![Again](demo.mp4) ![Shot](demo.png \"demo.mp4\")";

        let captured = store.capture_new(text, &cwd, &[]);

        let [video, shot] = &captured[..] else { panic!("a video and an image: {captured:?}") };
        assert_eq!((video.width, video.height), (Some(1280), Some(720)));
        assert_eq!(video.poster.as_ref(), Some(&shot.id), "the poster is the image's own copy");
        assert_eq!(shot.poster, None);
    }

    #[test]
    fn only_images_and_videos_are_kept_of_what_a_message_attaches() {
        let dir = tempfile::tempdir().unwrap();
        let (store, _) = store(&dir);
        let shot = dir.path().join("project/shot.png");
        let notes = dir.path().join("project/notes.txt");
        std::fs::write(&shot, png(10, 20)).unwrap();
        std::fs::write(&notes, b"notes").unwrap();
        let attached = [shot, notes].map(|path| path.to_string_lossy().into_owned());

        let captured = store.capture_attached(&attached);

        assert_eq!(captured.iter().map(|media| media.src.as_str()).collect::<Vec<_>>(), [attached[0].as_str()]);
    }
}
