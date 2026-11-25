//! The images and videos of the open thread, which its Media tab shows, and what can be done with
//! one from there or the viewer. `Gallery.swift`.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;
use motile_core::gallery::Shown;

use super::Store;
use crate::media::MediaFiles;
use crate::models::{MediaSource, ShownIn, ViewedMedia};

/// The images and videos of one thread, the newest first.
pub struct Gallery {
    pub thread_id: String,
    pub media: Rc<Vec<Shown>>,
}

/// The image a square shows: the image itself, or a video's poster.
pub fn picture(shown: &Shown) -> Option<&str> {
    if shown.video { shown.poster.as_deref() } else { Some(&shown.media) }
}

pub fn viewed(shown: &Shown) -> ViewedMedia {
    ViewedMedia {
        name: shown.name.clone(),
        video: shown.video,
        source: MediaSource::Media(shown.media.clone()),
        attachment: None,
        opens_at: None,
        pixels: shown.width.zip(shown.height),
        shown_in: Some(shown_in(shown)),
    }
}

pub fn shown_in(shown: &Shown) -> ShownIn {
    ShownIn { item: shown.item.clone(), row: shown.row.clone() }
}

/// "20 images · 1 video", leaving out what there is none of.
pub fn summary(media: &[Shown]) -> String {
    let videos = media.iter().filter(|shown| shown.video).count();
    [(media.len() - videos, "image"), (videos, "video")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, word)| format!("{count} {word}{}", if count == 1 { "" } else { "s" }))
        .collect::<Vec<_>>()
        .join(" · ")
}

impl Store {
    pub(super) fn apply_gallery(&mut self, thread_id: String, media: Vec<Shown>, cx: &App) {
        if self.transcript.read(cx).thread_id.as_deref() != Some(&thread_id) {
            return;
        }
        self.gallery = Some(Gallery { thread_id, media: Rc::new(media) });
    }

    /// Has the core send the thread's images and videos, and again whenever one is added.
    pub fn watch_gallery(&mut self, thread_id: &str, watch: bool) {
        if !watch && self.gallery.as_ref().is_some_and(|gallery| gallery.thread_id == thread_id) {
            self.gallery = None;
        }
        self.send(Command::WatchGallery { thread_id: thread_id.to_string(), watch });
    }

    /// Scrolls the open thread to where it shows the image or the video, out from behind the
    /// viewer and the panel.
    pub fn show_in_chat(&mut self, shown_in: &ShownIn, cx: &App) {
        let Some(thread_id) = self.transcript.read(cx).thread_id.clone() else { return };
        self.uncover_thread();
        self.send(Command::RevealRow { thread_id, item_id: shown_in.item.clone(), row_id: shown_in.row.clone() });
    }

    /// Takes the viewer and the maximized panel away from over the thread.
    pub fn uncover_thread(&mut self) {
        if self.viewing.is_some() {
            self.close_viewer();
        }
        if self.panel_maximized() {
            self.toggle_panel_maximized();
        }
    }

    /// Puts the image or the video in the composer, under its name.
    pub fn add_to_message(&mut self, media: &str, name: &str, cx: &mut Context<Self>) {
        let (store, weak, name) = (cx.entity(), cx.entity().downgrade(), name.to_string());
        MediaFiles::fetch(&store, media, cx, move |file, cx| {
            let Ok(file) = file else { return };
            let copying = cx.background_executor().spawn(async move { attachable_copy(&file, &name) });
            cx.spawn(async move |cx| {
                let copy = copying.await;
                let _ = weak.update(cx, |store, cx| {
                    store.uncover_thread();
                    store.attach(vec![copy]);
                    cx.notify();
                });
            })
            .detach();
        });
    }
}

/// A copy of the file under its name, in a folder of its own, so that two of the same name are
/// both attached.
fn attachable_copy(file: &Path, name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join("motile-attachments/gallery").join(uuid::Uuid::new_v4().to_string());
    let Some(named) = Path::new(name).file_name().or_else(|| file.file_name()) else { return file.to_path_buf() };
    let copy = folder.join(named);
    if std::fs::create_dir_all(&folder).is_err() || std::fs::copy(file, &copy).is_err() {
        return file.to_path_buf();
    }
    copy
}

#[cfg(test)]
mod tests {
    use super::{MediaSource, Shown, ShownIn, attachable_copy, picture, summary, viewed};

    fn shown(media: &str, video: bool) -> Shown {
        Shown {
            media: media.into(),
            row: "r/1".into(),
            item: "r".into(),
            name: media.into(),
            video,
            poster: video.then(|| "poster.png".into()),
            width: Some(4),
            height: Some(3),
            size: 10,
            at: 1.,
        }
    }

    #[test]
    fn the_summary_counts_images_and_videos_leaving_out_what_there_is_none_of() {
        assert_eq!(
            summary(&[shown("a.png", false), shown("b.png", false), shown("c.mp4", true)]),
            "2 images · 1 video"
        );
        assert_eq!(summary(&[shown("a.png", false)]), "1 image");
        assert_eq!(summary(&[shown("c.mp4", true), shown("d.mp4", true)]), "2 videos");
    }

    #[test]
    fn a_video_is_pictured_by_its_poster_and_opens_where_it_is_shown() {
        let video = shown("c.mp4", true);
        assert_eq!(picture(&video), Some("poster.png"));
        assert_eq!(picture(&shown("a.png", false)), Some("a.png"));

        let viewed = viewed(&video);
        assert_eq!(viewed.source, MediaSource::Media("c.mp4".into()));
        assert_eq!(viewed.shown_in, Some(ShownIn { item: "r".into(), row: "r/1".into() }));
        assert_eq!(viewed.pixels, Some((4, 3)));
    }

    #[test]
    fn a_file_is_attached_as_a_copy_under_its_name_apart_from_others_of_that_name() {
        let source = std::env::temp_dir().join(format!("motile-gallery-test-{}", uuid::Uuid::new_v4()));
        std::fs::write(&source, b"picture").unwrap();

        let (one, other) = (attachable_copy(&source, "shot.png"), attachable_copy(&source, "shot.png"));

        assert_eq!(one.file_name().unwrap(), "shot.png");
        assert_ne!(one, other);
        assert_eq!(std::fs::read(&other).unwrap(), b"picture");
        for file in [&source, &one, &other] {
            let _ = std::fs::remove_file(file);
        }
    }
}
