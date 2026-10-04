//! Files attached to what is being written, and the images and videos the threads show.

use std::path::PathBuf;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::api::Command;

use super::{MediaStorage, Store};
use crate::models::*;

impl Store {
    /// The file of an image or a video the open thread shows. The core fetches it from the
    /// thread's server if this device doesn't have it.
    pub fn media(&mut self, id: &str, done: impl FnOnce(&mut Store, Option<PathBuf>, &mut Context<Store>) + 'static) {
        let server_id = self
            .selected_thread()
            .map(|thread| thread.server_id.clone())
            .or_else(|| self.composer_server().map(|server| server.id.clone()));
        let Some(server_id) = server_id else { return };
        self.ask(Command::Media { server_id, media_id: id.to_string() }, move |store, result, cx| {
            let path = result.ok().and_then(|value| value["path"].as_str().map(PathBuf::from));
            done(store, path, cx);
        });
    }

    pub fn refresh_media_storage(&mut self) {
        self.ask(Command::Storage, |store, result, _| {
            let Ok(value) = result else { return };
            store.media_storage = Some(MediaStorage {
                used: value["media_bytes"].as_u64().unwrap_or(0),
                limit: value["media_limit"].as_u64().unwrap_or(0),
            });
        });
    }

    /// Removes the images and videos kept on this device. The servers still have them.
    pub fn clear_media(&mut self) {
        self.ask(Command::ClearMedia, |store, _, _| store.refresh_media_storage());
    }

    /// Adds the files to what is being written and starts sending them to its server, so they
    /// are there when the message is sent.
    pub fn attach(&mut self, files: Vec<PathBuf>) {
        let Some(server_id) = self.composer_server().map(|server| server.id.clone()) else { return };
        let key = self.draft_key();
        for file in files {
            if !file.is_file() || self.attachments().iter().any(|attachment| attachment.file.as_ref() == Some(&file)) {
                continue;
            }
            let attachment = Attachment::from_file(file, &server_id);
            let id = attachment.id.clone();
            self.attachments_by_key.entry(key.clone()).or_default().push(attachment);
            self.upload(&id);
        }
    }

    /// Attaches an image that was pasted, writing it to a file first.
    pub fn attach_image(&mut self, image: &Image) {
        let extension = match image.format {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Gif => "gif",
            ImageFormat::Webp => "webp",
            ImageFormat::Tiff => "tiff",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Svg => "svg",
            _ => "png",
        };
        if let Some(file) = save_for_attaching(&image.bytes, extension) {
            self.attach(vec![file]);
        }
    }

    pub fn remove_attachment(&mut self, id: &str) {
        let uploading = self
            .attachments()
            .iter()
            .any(|attachment| attachment.id == id && matches!(attachment.state, UploadState::Uploading(_)));
        if uploading {
            self.send(Command::CancelUpload { key: id.to_string() });
        }
        let key = self.draft_key();
        if let Some(attached) = self.attachments_by_key.get_mut(&key) {
            attached.retain(|attachment| attachment.id != id);
            if attached.is_empty() {
                self.attachments_by_key.remove(&key);
            }
        }
    }

    pub fn retry_attachment(&mut self, id: &str) {
        self.upload(id);
    }

    pub(super) fn change_attachment(&mut self, id: &str, change: impl FnOnce(&mut Attachment)) {
        for attached in self.attachments_by_key.values_mut() {
            if let Some(attachment) = attached.iter_mut().find(|attachment| attachment.id == id) {
                change(attachment);
                return;
            }
        }
    }

    fn find_attachment(&self, id: &str) -> Option<Attachment> {
        self.attachments_by_key.values().flatten().find(|attachment| attachment.id == id).cloned()
    }

    fn upload(&mut self, id: &str) {
        let Some(attachment) = self.find_attachment(id) else { return };
        let Some(file) = attachment.file.clone() else { return };
        let server_id = attachment.server_id.clone();
        self.change_attachment(id, |attachment| attachment.state = UploadState::Uploading(0.));
        let command = Command::Upload {
            server_id: server_id.clone(),
            key: id.to_string(),
            file: file.to_string_lossy().to_string(),
            poster_of: None,
        };
        let id = id.to_string();
        self.ask(command, move |store, result, cx| match result {
            Err(error) => store.change_attachment(&id, |attachment| {
                // An upload that was stopped to go to another server is on its way there.
                if attachment.server_id == server_id {
                    attachment.state = UploadState::Failed(error);
                }
            }),
            Ok(value) => {
                let path = value["path"].as_str().unwrap_or_default().to_string();
                let media = value["media"].as_str().map(String::from);
                let shown = media.is_some();
                store.change_attachment(&id, |attachment| {
                    attachment.path = Some(path.clone());
                    attachment.media = media;
                });
                if !(attachment.video && shown) {
                    return store.change_attachment(&id, |attachment| attachment.state = UploadState::Ready);
                }
                store.upload_poster(id, file, path, server_id, cx);
            }
        });
    }

    /// Sends the first frame of a video after it, which stands for it wherever it isn't played.
    /// A video without one is sent all the same.
    fn upload_poster(&mut self, id: String, file: PathBuf, path: String, server_id: String, cx: &mut Context<Self>) {
        let frame = cx.background_executor().spawn(async move { crate::media::poster::first_frame(&file, 1280) });
        cx.spawn(async move |this, cx| {
            let poster = frame.await;
            let _ = this.update(cx, |store, cx| {
                let Some(poster) = poster else { return store.poster_ready(&id, &path, None) };
                let command = Command::Upload {
                    server_id,
                    key: format!("{id}.poster"),
                    file: poster.to_string_lossy().to_string(),
                    poster_of: Some(path.clone()),
                };
                store.ask(command, move |store, result, _| {
                    let _ = std::fs::remove_file(&poster);
                    let media = result.ok().and_then(|value| value["media"].as_str().map(String::from));
                    store.poster_ready(&id, &path, media);
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn poster_ready(&mut self, id: &str, path: &str, poster: Option<String>) {
        self.change_attachment(id, |attachment| {
            if attachment.path.as_deref() == Some(path) {
                attachment.poster = poster;
                attachment.state = UploadState::Ready;
            }
        });
    }

    /// Sends the open draft's files again when its project is on another server than they are.
    pub(super) fn upload_to_composer_server(&mut self) {
        let Some(server_id) = self.composer_server().map(|server| server.id.clone()) else { return };
        let moving: Vec<String> = self
            .attachments()
            .iter()
            .filter(|attachment| attachment.server_id != server_id && attachment.file.is_some())
            .map(|attachment| attachment.id.clone())
            .collect();
        for id in moving {
            self.send(Command::CancelUpload { key: id.clone() });
            let server = server_id.clone();
            self.change_attachment(&id, |attachment| {
                attachment.server_id = server;
                attachment.path = None;
                attachment.media = None;
                attachment.poster = None;
            });
            self.upload(&id);
        }
    }

    // Viewer

    pub fn view(&mut self, media: Vec<ViewedMedia>, index: usize) {
        if index >= media.len() {
            return;
        }
        self.viewing = Some(Viewing { items: media, index });
    }

    /// Shows the image or video before or after the one that is shown, around the ends.
    pub fn view_next(&mut self, step: isize) {
        let Some(viewing) = self.viewing.as_mut() else { return };
        let count = viewing.items.len() as isize;
        if count < 2 {
            return;
        }
        viewing.index = ((viewing.index as isize + step + count) % count) as usize;
    }

    pub fn close_viewer(&mut self) {
        self.viewing = None;
        self.composer_focus += 1;
    }
}

/// Writes a pasted or dropped image to a file, which is what gets attached.
fn save_for_attaching(bytes: &[u8], extension: &str) -> Option<PathBuf> {
    let folder = std::env::temp_dir().join("motile-attachments");
    std::fs::create_dir_all(&folder).ok()?;
    let stamp = chrono_like_stamp();
    let file = folder.join(format!("Image {stamp}.{extension}"));
    std::fs::write(&file, bytes).ok()?;
    Some(file)
}

/// "2026-10-04 at 14.03.27.512", as the Mac names a screenshot.
fn chrono_like_stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d at %H.%M.%S%.3f").to_string()
}
