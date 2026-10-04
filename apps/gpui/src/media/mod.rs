//! Images and videos: the files of the ones the threads show, as the core fetches them, the rows
//! and tiles that show them, posters for the videos that are attached, and playing them.

pub mod poster;
#[cfg(target_os = "macos")]
pub mod video;
pub mod viewer;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::RowKind;

use crate::models::{AttachedFile, MediaSource, ViewedMedia};
use crate::store::Store;
use crate::theme::colors;
use crate::transcript::model::RowModel;
use crate::transcript::rows::RowContext;
use crate::ui::icons;
use crate::ui::menu::Menu;

/// The tallest an image or a video of a reply is shown.
const MAX_HEIGHT: f32 = 480.;
const GAP: f32 = 6.;
const TILE: (f32, f32) = (104., 78.);
const TILE_GAP: f32 = 6.;
const PLAYED_HERE: [&str; 3] = ["mp4", "mov", "m4v"];

/// The files of the images and videos asked for, by the name the core has them under.
#[derive(Default)]
pub struct MediaFiles {
    paths: HashMap<String, PathBuf>,
    asked: HashSet<String>,
    failed: HashSet<String>,
}

impl Global for MediaFiles {}

impl MediaFiles {
    pub fn path(id: &str, cx: &App) -> Option<PathBuf> {
        cx.try_global::<MediaFiles>()?.paths.get(id).cloned()
    }

    pub fn failed(id: &str, cx: &App) -> bool {
        cx.try_global::<MediaFiles>().is_some_and(|files| files.failed.contains(id))
    }

    /// The file of the image or video, once it is on this device; it is fetched the first time
    /// it is asked for, and whoever watches the store is told when it is there.
    pub fn fetch(store: &Entity<Store>, id: &str, cx: &mut App) -> Option<PathBuf> {
        Self::fetch_then(store, id, cx, |_, _| {})
    }

    pub fn fetch_then(
        store: &Entity<Store>,
        id: &str,
        cx: &mut App,
        then: impl FnOnce(Option<PathBuf>, &mut App) + 'static,
    ) -> Option<PathBuf> {
        if let Some(path) = Self::path(id, cx) {
            return Some(path);
        }
        let files = cx.default_global::<MediaFiles>();
        files.failed.remove(id);
        if !files.asked.insert(id.to_string()) {
            return None;
        }
        let (store, id) = (store.clone(), id.to_string());
        cx.defer(move |cx| {
            store.update(cx, |store, _| {
                store.media(&id.clone(), move |_, path, cx| {
                    let files = cx.default_global::<MediaFiles>();
                    files.asked.remove(&id);
                    match &path {
                        Some(path) => {
                            files.paths.insert(id, path.clone());
                        }
                        None => {
                            files.failed.insert(id);
                        }
                    }
                    then(path, cx);
                    cx.notify();
                });
            });
        });
        None
    }

    /// Forgets the files, which the core has removed.
    pub fn clear(cx: &mut App) {
        cx.set_global(MediaFiles::default());
    }
}

/// An image the core has or fetches, drawn at a size.
#[derive(IntoElement)]
pub struct MediaImage {
    store: Entity<Store>,
    id: String,
    width: f32,
    height: f32,
    cover: bool,
    radius: f32,
}

impl MediaImage {
    pub fn new(store: Entity<Store>, id: String) -> Self {
        Self { store, id, width: 64., height: 64., cover: false, radius: 0. }
    }

    pub fn size(mut self, side: f32) -> Self {
        self.width = side;
        self.height = side;
        self
    }

    pub fn frame(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn cover(mut self) -> Self {
        self.cover = true;
        self
    }
}

impl RenderOnce for MediaImage {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let Some(path) = MediaFiles::fetch(&self.store, &self.id, cx) else {
            return div().w(px(self.width)).h(px(self.height)).rounded(px(self.radius)).bg(c.bubble).into_any_element();
        };
        img(path)
            .w(px(self.width))
            .h(px(self.height))
            .rounded(px(self.radius))
            .object_fit(if self.cover { ObjectFit::Cover } else { ObjectFit::Contain })
            .into_any_element()
    }
}

/// The images, videos and files attached to a message: tiles of one size for the pictures, which
/// open in the viewer, and the names of the others under them.
pub fn attached_files(store: &Entity<Store>, files: &[AttachedFile], cx: &App) -> AnyElement {
    let c = colors(cx);
    let pictured: Vec<&AttachedFile> = files.iter().filter(|file| file.media.is_some()).collect();
    let viewed: Vec<ViewedMedia> = pictured
        .iter()
        .filter_map(|file| {
            file.media.as_ref().map(|media| ViewedMedia {
                name: file.name.clone(),
                video: file.video,
                source: MediaSource::Media(media.clone()),
            })
        })
        .collect();
    let names: Vec<String> =
        files.iter().filter(|file| file.media.is_none()).map(|file| format!("📎 {}", file.name)).collect();
    div()
        .flex()
        .flex_col()
        .gap(px(TILE_GAP))
        .when(!pictured.is_empty(), |files| {
            files.child(div().flex().flex_wrap().gap(px(TILE_GAP)).children(pictured.iter().enumerate().map(
                |(index, file)| {
                    let (store, viewed) = (store.clone(), viewed.clone());
                    div()
                        .id(SharedString::from(format!("attached-{}-{index}", file.media.clone().unwrap_or_default())))
                        .relative()
                        .w(px(TILE.0))
                        .h(px(TILE.1))
                        .rounded(px(10.))
                        .overflow_hidden()
                        .bg(c.code_background)
                        .border_1()
                        .border_color(c.border)
                        .cursor_pointer()
                        .when_some(file.picture().cloned(), |tile, picture| {
                            tile.child(MediaImage::new(store.clone(), picture).frame(TILE.0, TILE.1).cover())
                        })
                        .when(file.video, |tile| {
                            tile.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icons::symbol("play.circle.fill", 26.).text_color(white())),
                            )
                        })
                        .tooltip(crate::ui::tooltip(file.name.clone()))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            let viewed = viewed.clone();
                            store.update(cx, |store, cx| {
                                store.view(viewed, index);
                                cx.notify();
                            })
                        })
                },
            )))
        })
        .when(!names.is_empty(), |files| {
            files.child(div().h(px(16.)).text_size(px(12.)).text_color(c.secondary).truncate().child(names.join("   ")))
        })
        .into_any_element()
}

/// The box an image or a video is shown in, in a column: its own size, or smaller to fit, at most
/// `MAX_HEIGHT` tall. A video's size isn't known, so it is 16 by 9.
fn boxed(width: Option<u32>, height: Option<u32>) -> (f32, f32) {
    match (width, height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => {
            let (width, height) = (width as f32, height as f32);
            let scale = (MAX_HEIGHT / height).min(1.);
            (width * scale, height * scale)
        }
        _ => (640., 360.),
    }
}

fn file_size(bytes: u64) -> String {
    crate::composer::attachments::file_size(bytes)
}

/// An image or a video a reply shows. The core says how large it is, so the row has its place
/// before the file is on this device.
pub fn media_row(model: &RowModel, ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let RowKind::Media { media, video, width, height, size, alt, name } = &model.row.kind else {
        return div().into_any_element();
    };
    let (box_width, box_height) = boxed(*width, *height);
    let row_id = model.row.id.clone();
    let player = ctx.view.upgrade().and_then(|view| view.read(cx).player(&row_id));
    let fetching = ctx.view.upgrade().is_some_and(|view| view.read(cx).fetching(&row_id));
    let progress = ctx.store.read(cx).media_progress.get(media).copied();
    let path = (!video).then(|| MediaFiles::fetch_path_quietly(media, cx)).flatten();
    let failed = MediaFiles::failed(media, cx);
    let caption = if *video {
        if fetching {
            match progress {
                Some(fraction) => format!("Downloading {name} · {}%", (fraction * 100.) as u32),
                None => format!("Downloading {name}…"),
            }
        } else if failed {
            "This video couldn't be loaded. Click to try again.".to_string()
        } else {
            format!("{name} · {}", file_size(*size))
        }
    } else if failed {
        "This image couldn't be loaded. Click to try again.".to_string()
    } else {
        String::new()
    };
    let ratio = box_width / box_height;
    let (store, view) = (ctx.store.clone(), ctx.view.clone());
    let (media_id, name_owned, is_video, click_row) = (media.clone(), name.clone(), *video, row_id.clone());
    let (menu_store, menu_media, menu_name) = (ctx.store.clone(), media.clone(), name.clone());
    if !video && path.is_none() && !failed {
        let _ = MediaFiles::fetch_path_quietly(media, cx);
    }
    let picture: AnyElement = match (&player, &path) {
        (Some(player), _) => player.clone().into_any_element(),
        (None, Some(path)) => img(path.clone()).size_full().object_fit(ObjectFit::Contain).into_any_element(),
        _ => div().into_any_element(),
    };
    let store_for_image = ctx.store.clone();
    let wants_image = !video && path.is_none() && !failed;
    div()
        .py(px(GAP))
        .child(
            div()
                .id(SharedString::from(format!("media-{row_id}")))
                .relative()
                .w_full()
                .max_w(px(box_width))
                .aspect_ratio(ratio)
                .rounded(px(10.))
                .overflow_hidden()
                .bg(c.code_background)
                .border_1()
                .border_color(c.border)
                .child(picture)
                .when(wants_image, |frame| {
                    let store = store_for_image.clone();
                    let id = media.clone();
                    frame.child(canvas(
                        move |_, _, cx| {
                            let _ = MediaFiles::fetch(&store, &id, cx);
                        },
                        |_, _, _, _| {},
                    ))
                })
                .when(*video && player.is_none(), |frame| {
                    frame.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::symbol("play.circle.fill", 40.).text_color(c.secondary)),
                    )
                })
                .when(!caption.is_empty() && player.is_none(), |frame| {
                    frame.child(
                        div()
                            .absolute()
                            .left(px(12.))
                            .right(px(12.))
                            .bottom(px(10.))
                            .h(px(16.))
                            .text_size(px(12.))
                            .text_color(c.secondary)
                            .truncate()
                            .child(caption),
                    )
                })
                .when(!alt.is_empty(), |frame| frame.tooltip(crate::ui::tooltip(alt.clone())))
                .when(player.is_none(), |frame| {
                    frame.on_click(move |_, _, cx| {
                        if !is_video {
                            if MediaFiles::path(&media_id, cx).is_none() {
                                let _ = MediaFiles::fetch(&store, &media_id, cx);
                                return;
                            }
                            let viewed = vec![ViewedMedia {
                                name: name_owned.clone(),
                                video: false,
                                source: MediaSource::Media(media_id.clone()),
                            }];
                            store.update(cx, |store, cx| {
                                store.view(viewed, 0);
                                cx.notify();
                            });
                            return;
                        }
                        let _ = view.update(cx, |view, cx| view.play(click_row.clone(), media_id.clone(), cx));
                    })
                })
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    let (copy_store, save_store) = (menu_store.clone(), menu_store.clone());
                    let (copy_id, save_id, save_name) = (menu_media.clone(), menu_media.clone(), menu_name.clone());
                    Menu::new()
                        .when(!is_video, |menu| {
                            menu.item("Copy Image", move |_, cx| {
                                let copy_id = copy_id.clone();
                                let path = MediaFiles::fetch_then(&copy_store, &copy_id, cx, |path, cx| {
                                    if let Some(path) = path {
                                        copy_image(&path, cx);
                                    }
                                });
                                if let Some(path) = path {
                                    copy_image(&path, cx);
                                }
                            })
                        })
                        .item("Save As…", move |_, cx| {
                            let name = save_name.clone();
                            let path = MediaFiles::fetch_then(&save_store, &save_id, cx, move |path, cx| {
                                if let Some(path) = path {
                                    save_as(path, name, cx);
                                }
                            });
                            if let Some(path) = path {
                                save_as(path, save_name.clone(), cx);
                            }
                        })
                        .show(event.position, window, cx);
                }),
        )
        .into_any_element()
}

impl MediaFiles {
    /// The file if it is here, without asking for it.
    fn fetch_path_quietly(id: &str, cx: &App) -> Option<PathBuf> {
        Self::path(id, cx)
    }
}

fn image_format(path: &Path) -> ImageFormat {
    match path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase().as_str() {
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "gif" => ImageFormat::Gif,
        "webp" => ImageFormat::Webp,
        "svg" => ImageFormat::Svg,
        "bmp" => ImageFormat::Bmp,
        "tif" | "tiff" => ImageFormat::Tiff,
        _ => ImageFormat::Png,
    }
}

fn copy_image(path: &Path, cx: &mut App) {
    let Ok(bytes) = std::fs::read(path) else { return };
    cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(image_format(path), bytes)));
}

/// Asks where to save the file and copies it there.
fn save_as(path: PathBuf, name: String, cx: &mut App) {
    let folder = std::env::var("HOME").map(|home| PathBuf::from(home).join("Downloads")).unwrap_or_default();
    let chosen = cx.prompt_for_new_path(&folder, Some(&name));
    cx.background_executor()
        .spawn(async move {
            let Ok(Ok(Some(destination))) = chosen.await else { return };
            let _ = std::fs::remove_file(&destination);
            let _ = std::fs::copy(&path, &destination);
        })
        .detach();
}

/// Whether the system's player is used for a video here, or the app the system has for it.
pub fn plays_here(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    PLAYED_HERE.contains(&extension.as_str())
}
