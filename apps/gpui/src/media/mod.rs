//! Images, videos and files: the files of the ones the threads show, as the core fetches them, the
//! rows and tiles that show them, posters for the videos that are attached, and playing them.

pub mod file;
pub mod native;
pub mod pictures;
pub mod poster;
#[cfg(target_os = "macos")]
pub mod video;
pub mod viewer;
pub mod zoom;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::RowKind;
use pictures::{Failure, Picture, Pictures};

use crate::models::{AttachedFile, MediaSource, ViewedMedia};
use crate::store::Store;
use crate::theme::{ControlSize, Radius, colors};
use crate::transcript::kit::RowButton;
use crate::transcript::model::RowModel;
use crate::transcript::rows::RowContext;
use crate::ui::menu::Menu;
use crate::ui::{ActionButton, Spinner};

/// The tallest an image or a video of a reply is shown.
const MAX_HEIGHT: f32 = 480.;
const GAP: f32 = 6.;
/// The room between the spinner and its percentage, and the message and its button.
const STACK_GAP: f32 = 10.;
const LINE_HEIGHT: f32 = 16.;
const TILE: f32 = 96.;
const TILE_GAP: f32 = 6.;
const PLAYED_HERE: [&str; 3] = ["mp4", "mov", "m4v"];
pub const SAVE_TITLE: &str = "Save As";

type Waiter = Box<dyn FnOnce(Result<PathBuf, String>, &mut App)>;

/// The files of the images, videos and files asked for, by the name the core has them under,
/// and what waits for the ones on their way.
#[derive(Default)]
pub struct MediaFiles {
    paths: HashMap<String, PathBuf>,
    waiting: HashMap<String, (u64, Vec<Waiter>)>,
    asks: u64,
}

impl Global for MediaFiles {}

impl MediaFiles {
    pub fn path(id: &str, cx: &App) -> Option<PathBuf> {
        cx.try_global::<MediaFiles>()?.paths.get(id).cloned()
    }

    /// Hands over the file, or why it can't be had, once the core has it on this device. It is
    /// fetched from the server the first time it is asked for.
    pub fn fetch(
        store: &Entity<Store>,
        id: &str,
        cx: &mut App,
        done: impl FnOnce(Result<PathBuf, String>, &mut App) + 'static,
    ) {
        if let Some(path) = Self::path(id, cx) {
            return cx.defer(move |cx| done(Ok(path), cx));
        }
        let files = cx.default_global::<MediaFiles>();
        if let Some((_, waiters)) = files.waiting.get_mut(id) {
            return waiters.push(Box::new(done));
        }
        files.asks += 1;
        let ask = files.asks;
        files.waiting.insert(id.to_string(), (ask, vec![Box::new(done)]));
        let (store, id) = (store.clone(), id.to_string());
        cx.defer(move |cx| {
            store.update(cx, |store, cx| {
                store.media_progress.remove(&id);
                store.media(
                    &id.clone(),
                    move |_, result, cx| {
                        let files = cx.default_global::<MediaFiles>();
                        if files.waiting.get(&id).is_none_or(|(asked, _)| *asked != ask) {
                            return;
                        }
                        let (_, waiters) = files.waiting.remove(&id).unwrap_or_default();
                        if let Ok(path) = &result {
                            files.paths.insert(id, path.clone());
                        }
                        for waiter in waiters {
                            waiter(result.clone(), cx);
                        }
                        cx.refresh_windows();
                    },
                    cx,
                );
            });
        });
    }

    /// Stops fetching the file. What waits for it is told that it stopped.
    pub fn cancel(store: &Entity<Store>, id: &str, cx: &mut App) {
        let Some((_, waiters)) = cx.default_global::<MediaFiles>().waiting.remove(id) else { return };
        store.update(cx, |store, _| store.cancel_media(id));
        for waiter in waiters {
            waiter(Err("The download was stopped.".into()), cx);
        }
    }

    /// Forgets the files, which the core has removed.
    pub fn clear(cx: &mut App) {
        cx.set_global(MediaFiles::default());
    }
}

/// An image the core has or fetches, fitted in a frame.
#[derive(IntoElement)]
pub struct MediaImage {
    store: Entity<Store>,
    id: String,
    width: f32,
    height: f32,
    radius: f32,
}

impl MediaImage {
    pub fn new(store: Entity<Store>, id: String) -> Self {
        Self { store, id, width: 64., height: 64., radius: 0. }
    }

    pub fn size(mut self, side: f32) -> Self {
        self.width = side;
        self.height = side;
        self
    }

    /// Its corners, which a box around it doesn't round.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl RenderOnce for MediaImage {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let max_pixels = (self.width.max(self.height) * 2. * window.scale_factor()).ceil() as u32;
        let picture = Pictures::load(&self.store, &format!("tile:{}", self.id), &self.id, max_pixels, cx);
        let c = colors(cx);
        let Picture::Ready(shown) = picture else {
            return div().w(px(self.width)).h(px(self.height)).bg(c.background_secondary).into_any_element();
        };
        // The box's own shape, which GPUI would otherwise take from the image.
        img(ImageSource::Render(shown.image))
            .w(px(self.width))
            .h(px(self.height))
            .aspect_ratio(self.width / self.height)
            .rounded(px(self.radius))
            .object_fit(ObjectFit::Contain)
            .into_any_element()
    }
}

/// What a right click on an image or a video offers: copying it and saving it.
pub fn media_menu(store: &Entity<Store>, id: &str, video: bool, name: &str) -> Menu {
    let (copy_store, save_store) = (store.clone(), store.clone());
    let (copy_id, save_id) = (id.to_string(), id.to_string());
    let (copy_name, save_name) = (name.to_string(), name.to_string());
    Menu::new()
        .symbol_item(if video { "Copy Video" } else { "Copy Image" }, "copy", move |_, cx| {
            let name = copy_name.clone();
            MediaFiles::fetch(&copy_store, &copy_id, cx, move |file, cx| {
                if let Ok(file) = file {
                    copy(file, video, name, cx);
                }
            });
        })
        .symbol_item(SAVE_TITLE, "download", move |_, cx| {
            let name = save_name.clone();
            MediaFiles::fetch(&save_store, &save_id, cx, move |file, cx| {
                if let Ok(file) = file {
                    native::save(file, &name, cx);
                }
            });
        })
}

/// Copies an image's picture, or a video's file under its name.
pub fn copy(file: PathBuf, video: bool, name: String, cx: &mut App) {
    if video {
        let copying = cx.background_executor().spawn(async move { named_copy(&file, &name) });
        return cx
            .spawn(async move |cx| {
                let file = copying.await;
                cx.update(|cx| native::copy_file(&file, cx))
            })
            .detach();
    }
    let format = image_format(&file);
    let reading = cx.background_executor().spawn(async move { std::fs::read(&file).ok() });
    cx.spawn(async move |cx| {
        let Some(bytes) = reading.await else { return };
        cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(format, bytes))));
    })
    .detach();
}

/// A copy of the file under its name, in a temporary folder, for what takes it to know it by it.
pub fn named_copy(file: &Path, name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join("motile-shared");
    let named = if name.is_empty() { file.file_name().map(PathBuf::from).unwrap_or_default() } else { name.into() };
    let copy = folder.join(named);
    let _ = std::fs::create_dir_all(&folder);
    let _ = std::fs::remove_file(&copy);
    if std::fs::copy(file, &copy).is_ok() { copy } else { file.to_path_buf() }
}

/// The images, videos and files attached to a message: tiles of one size with the whole picture
/// fitted in each, which open in the viewer, and the names of the others under them.
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
                    let media = file.media.clone().unwrap_or_default();
                    let (store, viewed) = (store.clone(), viewed.clone());
                    let (open_store, open_viewed) = (store.clone(), viewed.clone());
                    let (menu_store, menu_media, menu_name, video) =
                        (store.clone(), media.clone(), file.name.clone(), file.video);
                    let open = move |cx: &mut App| {
                        let viewed = open_viewed.clone();
                        open_store.update(cx, |store, cx| {
                            store.view(viewed, index);
                            cx.notify();
                        })
                    };
                    let play = open.clone();
                    div()
                        .id(SharedString::from(format!("attached-{media}-{index}")))
                        .relative()
                        .size(px(TILE))
                        .rounded(px(Radius::LG))
                        .overflow_hidden()
                        .bg(c.background_secondary)
                        .border_1()
                        .border_color(c.border)
                        .cursor_pointer()
                        .when_some(file.picture().cloned(), |tile, picture| {
                            tile.child(MediaImage::new(store.clone(), picture).size(TILE - 2.).radius(Radius::LG - 1.))
                        })
                        .when(file.video, |tile| {
                            tile.child(
                                div().absolute().inset_0().flex().items_center().justify_center().child(
                                    ActionButton::icon(
                                        SharedString::from(format!("play-{media}-{index}")),
                                        "play",
                                        "Play",
                                    )
                                    .overlay()
                                    .round(true)
                                    .on_click(move |_, _, cx| play(cx)),
                                ),
                            )
                        })
                        .tooltip(crate::ui::tooltip(file.name.clone()))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            open(cx)
                        })
                        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                            cx.stop_propagation();
                            media_menu(&menu_store, &menu_media, video, &menu_name).show(event.position, window, cx);
                        })
                },
            )))
        })
        .when(!names.is_empty(), |files| {
            files.child(
                div()
                    .h(px(LINE_HEIGHT))
                    .text_size(px(12.))
                    .text_color(c.muted_foreground)
                    .truncate()
                    .child(names.join("   ")),
            )
        })
        .into_any_element()
}

/// The box an image or a video is shown in, in a column: its own size, or smaller to fit, at most
/// `MAX_HEIGHT` tall. A video's size isn't known, so it is 16 by 9.
fn boxed(width: Option<u32>, height: Option<u32>) -> (f32, f32) {
    match (width, height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => {
            let (width, height) = (width as f32, height as f32);
            let scale = (MAX_HEIGHT / height).min(crate::theme::CONTENT_WIDTH / width).min(1.);
            ((width * scale).round().max(1.), (height * scale).round().max(1.))
        }
        _ => (640., 360.),
    }
}

pub fn file_size(bytes: u64) -> String {
    crate::composer::attachments::file_size(bytes)
}

/// An image or a video a reply shows. The core says how large it is, so the row has its place
/// before the file is on this device. A video shows its poster, or the box with its name, and
/// either opens in the viewer.
pub fn media_row(model: &RowModel, ctx: &RowContext, window: &mut Window, cx: &mut App) -> AnyElement {
    let RowKind::Media { media, video, width, height, size, alt, name, poster } = &model.row.kind else {
        return div().into_any_element();
    };
    let (box_width, box_height) = boxed(*width, *height);
    let video = *video;
    let picture_id = if video { poster.clone() } else { Some(media.clone()) };
    let max_pixels = (box_width.max(box_height) * window.scale_factor()).ceil() as u32;
    let picture = picture_id.as_ref().map(|id| Pictures::load(&ctx.store, id, id, max_pixels, cx));
    let fraction = (!video).then(|| ctx.store.read(cx).media_progress.get(media).copied()).flatten();
    let c = colors(cx);
    let row_id = model.row.id.clone();
    let shown = match &picture {
        Some(Picture::Ready(shown)) => Some(shown.image.clone()),
        _ => None,
    };
    let failure = match &picture {
        Some(Picture::Failed(failure)) if !video => Some(failure.clone()),
        _ => None,
    };
    let loading = matches!(picture, Some(Picture::Loading));
    let caption = (video && poster.is_none()).then(|| format!("{name} · {}", file_size(*size)));
    let viewed = vec![ViewedMedia { name: name.clone(), video, source: MediaSource::Media(media.clone()) }];
    let open = {
        let store = ctx.store.clone();
        let ready = shown.is_some();
        move |cx: &mut App| {
            if !video && !ready {
                return;
            }
            let viewed = viewed.clone();
            store.update(cx, |store, cx| {
                store.view(viewed, 0);
                cx.notify();
            });
        }
    };
    let play = open.clone();
    let retry_key = picture_id.clone().unwrap_or_default();
    let (menu_store, menu_media, menu_name) = (ctx.store.clone(), media.clone(), name.clone());
    let retries = !ctx.removed;
    let message: SharedString = match failure.clone() {
        Some(Failure::Fetch(reason)) if ctx.removed => reason,
        Some(Failure::Fetch(_)) => "This image couldn't be loaded.".into(),
        Some(Failure::Decode) => "This image couldn't be shown.".into(),
        None => "".into(),
    };

    let states = div()
        .absolute()
        .inset_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .when(loading, |states| {
            states
                .child(
                    div()
                        .size(px(ControlSize::Large.symbol().side()))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(c.muted_foreground)
                        .child(Spinner::new(ControlSize::Large.symbol()).render(cx)),
                )
                .when_some(fraction, |states, fraction| {
                    states.child(
                        div()
                            .mt(px(STACK_GAP))
                            .h(px(LINE_HEIGHT))
                            .text_size(px(12.))
                            .font_features(crate::ui::tabular_numbers())
                            .text_color(c.muted_foreground)
                            .child(format!("{}%", (fraction * 100.) as u32)),
                    )
                })
        })
        .when(failure.is_some(), |states| {
            states
                .child(
                    div()
                        .max_w_full()
                        .px(px(12.))
                        .h(px(LINE_HEIGHT))
                        .text_size(px(12.))
                        .text_color(c.muted_foreground)
                        .truncate()
                        .child(message),
                )
                .when(retries, |states| {
                    states.child(
                        div().mt(px(STACK_GAP)).child(
                            RowButton::new(
                                SharedString::from(format!("retry-{row_id}")),
                                "Try again",
                                "Load the image again",
                            )
                            .bordered()
                            .hover(c.background_secondary_accent)
                            .on_click(move |_, _, cx| {
                                Pictures::retry(&retry_key, cx);
                                cx.refresh_windows();
                            }),
                        ),
                    )
                })
        });

    div()
        .py(px(GAP))
        .child(
            div()
                .id(SharedString::from(format!("media-{row_id}")))
                .relative()
                .w_full()
                .max_w(px(box_width))
                .aspect_ratio(box_width / box_height)
                .rounded(px(Radius::LG))
                .overflow_hidden()
                .bg(c.background_secondary)
                .border_1()
                .border_color(c.border)
                .when_some(shown, |frame, image| {
                    frame.child(
                        img(ImageSource::Render(image))
                            .size_full()
                            .aspect_ratio(box_width / box_height)
                            .rounded(px(Radius::LG - 1.))
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .child(states)
                .when(video, |frame| {
                    frame.child(
                        div().absolute().inset_0().flex().items_center().justify_center().child(
                            ActionButton::icon(SharedString::from(format!("play-{row_id}")), "play", "Play")
                                .overlay()
                                .large()
                                .round(true)
                                .on_click(move |_, _, cx| play(cx)),
                        ),
                    )
                })
                .when_some(caption, |frame, caption| {
                    frame.child(
                        div()
                            .absolute()
                            .left(px(11.))
                            .right(px(11.))
                            .bottom(px(9.))
                            .h(px(LINE_HEIGHT))
                            .text_size(px(12.))
                            .text_color(c.muted_foreground)
                            .child(crate::ui::middle_truncated(caption)),
                    )
                })
                .when(!alt.is_empty(), |frame| frame.tooltip(crate::ui::tooltip(alt.clone())))
                .on_click(move |_, _, cx| open(cx))
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    media_menu(&menu_store, &menu_media, video, &menu_name).show(event.position, window, cx);
                }),
        )
        .into_any_element()
}

pub fn image_format(path: &Path) -> ImageFormat {
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

/// Whether the system's player is used for a video here, or the app the system has for it.
pub fn plays_here(path: &Path) -> bool {
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or_default().to_lowercase();
    PLAYED_HERE.contains(&extension.as_str())
}
