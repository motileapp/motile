//! A file an agent sent, other than an image or a video, for the user to download: the card its
//! row shows, and the downloads, which go on while their rows are off screen and are remembered
//! across launches.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::RowKind;

use super::{MediaFiles, SAVE_TITLE, file_size, named_copy, native};
use crate::store::Store;
use crate::theme::{ControlSize, Radius, colors};
use crate::transcript::kit::{RowButton, progress_ring};
use crate::transcript::model::RowModel;
use crate::transcript::rows::RowContext;
use crate::ui::icons;
use crate::ui::menu::Menu;

const GAP: f32 = 6.;
const CARD_HEIGHT: f32 = 56.;
const WIDEST: f32 = 420.;
const TILE_SIDE: f32 = 36.;
const RING_INSET: f32 = 5.;
const INSET: f32 = (CARD_HEIGHT - TILE_SIDE) / 2.;
const LINE_HEIGHT: f32 = 16.;
/// The button is as far from the card's side as from its top and bottom.
const BUTTON_MARGIN: f32 = (CARD_HEIGHT - 28.) / 2.;
const REVEAL_TITLE: &str = crate::models::REVEAL_IN_FOLDER;
/// Where the downloads went, by the file's name on the server, in the preferences.
const REMEMBERED: &str = "downloaded_files";
/// How long the ring takes to grow to what has arrived.
const GROW: Duration = Duration::from_millis(250);

/// What the ring shows: it grows from `from` to `to` since `at`.
#[derive(Clone, Copy)]
struct Ring {
    from: f64,
    to: f64,
    at: Instant,
}

impl Ring {
    fn value(&self) -> f64 {
        let progress = (self.at.elapsed().as_secs_f64() / GROW.as_secs_f64()).min(1.);
        self.from + (self.to - self.from) * f64::from(crate::ui::ease_out(progress as f32))
    }
}

/// A file a reply sends.
#[derive(Clone)]
pub struct SentFile {
    pub id: String,
    pub bytes: u64,
    pub alt: String,
    pub name: String,
}

impl SentFile {
    fn kind(&self) -> String {
        Path::new(&self.name)
            .extension()
            .map(|extension| extension.to_string_lossy().to_uppercase())
            .unwrap_or_default()
    }

    /// What kind of file it is and how large, as "PDF · 2.4 MB".
    fn detail(&self) -> String {
        let kind = self.kind();
        if kind.is_empty() { file_size(self.bytes) } else { format!("{kind} · {}", file_size(self.bytes)) }
    }

    fn symbol(&self) -> &'static str {
        match self.kind().to_lowercase().as_str() {
            "pdf" | "md" | "markdown" | "txt" | "rst" | "csv" | "tsv" | "doc" | "docx" | "rtf" | "pages" | "odt"
            | "log" => "file-text",
            _ => "file",
        }
    }
}

/// Where a file's download is.
#[derive(Clone, PartialEq, Debug)]
pub enum Download {
    Remote,
    Fetching(f64),
    Failed(String),
    Downloaded(PathBuf),
}

/// The files agents sent that are downloading or were downloaded.
#[derive(Default)]
pub struct FileDownloads {
    fetching: HashSet<String>,
    /// Files fetched that are being put where downloads go.
    saving: HashSet<String>,
    failures: HashMap<String, String>,
    /// Goes up when a fetch is stopped, so that its answer is not taken for a later one's.
    generations: HashMap<String, u64>,
    /// Where each download went, while it is still there.
    downloaded: HashMap<String, PathBuf>,
    /// The rings of the files being fetched.
    rings: HashMap<String, Ring>,
    loaded: bool,
}

impl Global for FileDownloads {}

impl FileDownloads {
    pub fn state(store: &Entity<Store>, id: &str, cx: &mut App) -> Download {
        Self::load(store, cx);
        let downloads = cx.default_global::<FileDownloads>();
        if downloads.saving.contains(id) {
            return Download::Fetching(1.);
        }
        if downloads.fetching.contains(id) {
            return Download::Fetching(store.read(cx).media_progress.get(id).copied().unwrap_or(0.));
        }
        if let Some(file) = downloads.downloaded.get(id) {
            return Download::Downloaded(file.clone());
        }
        if let Some(failure) = downloads.failures.get(id) {
            return Download::Failed(failure.clone());
        }
        Download::Remote
    }

    /// The fraction a fetch's ring shows, and whether it is still growing. It starts empty, grows
    /// to what has arrived and goes back at once.
    fn ring(id: &str, fraction: f64, cx: &mut App) -> (f64, bool) {
        let now = Instant::now();
        let rings = &mut cx.default_global::<FileDownloads>().rings;
        let ring = rings.entry(id.to_string()).or_insert(Ring { from: 0., to: 0., at: now });
        if fraction != ring.to {
            let current = ring.value();
            *ring = if fraction < current {
                Ring { from: fraction, to: fraction, at: now }
            } else {
                Ring { from: current, to: fraction, at: now }
            };
        }
        (ring.value(), ring.at.elapsed() < GROW)
    }

    /// Reads where the downloads went, once, keeping those that are still there.
    fn load(store: &Entity<Store>, cx: &mut App) {
        if cx.default_global::<FileDownloads>().loaded {
            return;
        }
        let home = home();
        let remembered: HashMap<String, String> = store.read(cx).prefs.get(REMEMBERED).unwrap_or_default();
        let downloads = cx.default_global::<FileDownloads>();
        downloads.loaded = true;
        for (id, path) in remembered {
            let file = if path.starts_with('/') { PathBuf::from(path) } else { home.join(path) };
            if file.exists() {
                downloads.downloaded.insert(id, file);
            }
        }
    }

    /// Looks again whether the downloaded file is still where it went: it may have been moved.
    fn recheck(id: &str, cx: &mut App) {
        let downloads = cx.default_global::<FileDownloads>();
        if downloads.downloaded.get(id).is_some_and(|file| !file.exists()) {
            downloads.downloaded.remove(id);
            cx.refresh_windows();
        }
    }

    /// Fetches the file and puts it in Downloads, under its name or a free one like it.
    fn download(store: &Entity<Store>, file: &SentFile, cx: &mut App) {
        let (store_for_save, id, name) = (store.clone(), file.id.clone(), file.name.clone());
        Self::fetch(store, &file.id, cx, move |fetched, cx| {
            cx.default_global::<FileDownloads>().saving.insert(id.clone());
            let saving = cx.background_executor().spawn(async move { save_in_downloads(&fetched, &name) });
            cx.spawn(async move |cx| {
                let saved = saving.await;
                cx.update(|cx| {
                    let downloads = cx.default_global::<FileDownloads>();
                    downloads.saving.remove(&id);
                    match saved {
                        Some(saved) => {
                            native::download_finished(&saved);
                            remember(&store_for_save, &id, &saved, cx);
                            cx.default_global::<FileDownloads>().downloaded.insert(id, saved);
                        }
                        None => {
                            downloads.failures.insert(id, "This file couldn't be saved in Downloads.".into());
                        }
                    }
                    cx.refresh_windows();
                });
            })
            .detach();
        });
    }

    /// Hands over the client's copy of the file, which the core fetches if this device doesn't
    /// have it.
    fn fetch(store: &Entity<Store>, id: &str, cx: &mut App, done: impl FnOnce(PathBuf, &mut App) + 'static) {
        let downloads = cx.default_global::<FileDownloads>();
        let generation = downloads.generations.get(id).copied().unwrap_or(0);
        downloads.fetching.insert(id.to_string());
        downloads.failures.remove(id);
        let id = id.to_string();
        MediaFiles::fetch(store, &id.clone(), cx, move |result, cx| {
            let downloads = cx.default_global::<FileDownloads>();
            if downloads.generations.get(&id).copied().unwrap_or(0) != generation {
                return;
            }
            downloads.fetching.remove(&id);
            downloads.rings.remove(&id);
            match result {
                Ok(fetched) => done(fetched, cx),
                Err(error) => {
                    downloads.failures.insert(id, error);
                }
            }
            cx.refresh_windows();
        });
        cx.refresh_windows();
    }

    fn cancel(store: &Entity<Store>, id: &str, cx: &mut App) {
        let downloads = cx.default_global::<FileDownloads>();
        if !downloads.fetching.remove(id) {
            return;
        }
        downloads.rings.remove(id);
        *downloads.generations.entry(id.to_string()).or_default() += 1;
        MediaFiles::cancel(store, id, cx);
        cx.refresh_windows();
    }

    /// Hands over the downloaded file, or the client's copy, which the core fetches if need be.
    fn with_file(store: &Entity<Store>, id: &str, cx: &mut App, done: impl FnOnce(PathBuf, &mut App) + 'static) {
        if let Download::Downloaded(file) = Self::state(store, id, cx) {
            return done(file, cx);
        }
        Self::fetch(store, id, cx, done);
    }
}

fn home() -> PathBuf {
    std::env::var("HOME").map(PathBuf::from).unwrap_or_default()
}

/// Keeps where the file went, from the home folder.
fn remember(store: &Entity<Store>, id: &str, file: &Path, cx: &mut App) {
    let home = home();
    let path = file.strip_prefix(&home).unwrap_or(file).to_string_lossy().to_string();
    store.update(cx, |store, _| {
        let mut files: HashMap<String, String> = store.prefs.get(REMEMBERED).unwrap_or_default();
        files.insert(id.to_string(), path);
        store.prefs.set(REMEMBERED, files);
    });
}

/// Copies the file into Downloads under its name, or "name 2", "name 3" when that is taken.
fn save_in_downloads(file: &Path, name: &str) -> Option<PathBuf> {
    let folder = home().join("Downloads");
    let name = if name.is_empty() { file.file_name()?.to_string_lossy().to_string() } else { name.to_string() };
    let stem = Path::new(&name).file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    let extension = Path::new(&name).extension().map(|extension| extension.to_string_lossy().to_string());
    let mut candidate = folder.join(&name);
    let mut number = 2;
    while candidate.exists() {
        candidate = folder.join(match &extension {
            Some(extension) => format!("{stem} {number}.{extension}"),
            None => format!("{stem} {number}"),
        });
        number += 1;
    }
    std::fs::copy(file, &candidate).ok().map(|_| candidate)
}

/// The words that say what went wrong, without the system's reasons after them.
fn first_sentence(reason: &str) -> String {
    match reason.find(". ") {
        Some(end) => format!("{}.", &reason[..end]),
        None => reason.to_string(),
    }
}

fn menu(store: &Entity<Store>, file: &SentFile, state: &Download) -> Menu {
    let copy = {
        let (store, file) = (store.clone(), file.clone());
        move |_: &mut Window, cx: &mut App| {
            let name = file.name.clone();
            FileDownloads::with_file(&store, &file.id, cx, move |fetched, cx| {
                let copying = cx.background_executor().spawn(async move { named_copy(&fetched, &name) });
                cx.spawn(async move |cx| {
                    let file = copying.await;
                    cx.update(|cx| native::copy_file(&file, cx))
                })
                .detach();
            });
        }
    };
    match state {
        Download::Fetching(_) => {
            let (store, id) = (store.clone(), file.id.clone());
            Menu::new().symbol_item("Cancel Download", "x", move |_, cx| FileDownloads::cancel(&store, &id, cx))
        }
        Download::Downloaded(downloaded) => {
            let (opened, revealed) = (downloaded.clone(), downloaded.clone());
            Menu::new()
                .symbol_item("Open", "square-arrow-out-up-right", move |_, cx| cx.open_with_system(&opened))
                .symbol_item(REVEAL_TITLE, "folder", move |_, cx| cx.reveal_path(&revealed))
                .symbol_item("Copy File", "copy", copy)
        }
        Download::Remote | Download::Failed(_) => {
            let (download_store, download_file) = (store.clone(), file.clone());
            let (save_store, save_file) = (store.clone(), file.clone());
            Menu::new()
                .symbol_item("Download", "download", move |_, cx| {
                    FileDownloads::download(&download_store, &download_file, cx)
                })
                .symbol_item(SAVE_TITLE, "download", move |_, cx| {
                    let name = save_file.name.clone();
                    FileDownloads::with_file(&save_store, &save_file.id, cx, move |fetched, cx| {
                        native::save(fetched, &name, cx)
                    });
                })
                .symbol_item("Copy File", "copy", copy)
        }
    }
}

/// A file a reply sends, as a card with its name, its kind and size. Its icon says where the
/// download is, as a messaging app's does: an arrow to download it, the progress with a cross
/// that stops it, then the kind of file, which the card opens.
pub fn file_row(model: &RowModel, ctx: &RowContext, window: &mut Window, cx: &mut App) -> AnyElement {
    let RowKind::File { media, size, alt, name } = &model.row.kind else { return div().into_any_element() };
    let file = SentFile { id: media.clone(), bytes: *size, alt: alt.clone(), name: name.clone() };
    let store = ctx.store.clone();
    let state = FileDownloads::state(&store, media, cx);
    let card_id: ElementId = SharedString::from(format!("file-{}", model.row.id)).into();
    let tile_id: ElementId = (card_id.clone(), "tile").into();
    let reveal_id: ElementId = (card_id.clone(), "reveal").into();
    let card_hover = crate::ui::hover_state(&card_id, window, cx);
    let tile_hover = crate::ui::hover_state(&tile_id, window, cx);
    let reveal_hover = crate::ui::hover_state(&reveal_id, window, cx);
    let (over_card, over_tile, over_reveal) = (*card_hover.read(cx), *tile_hover.read(cx), *reveal_hover.read(cx));
    let ring = match state {
        Download::Fetching(fraction) => {
            let (shown, growing) = FileDownloads::ring(media, fraction, cx);
            if growing {
                window.request_animation_frame();
            }
            Some(shown)
        }
        _ => None,
    };
    let c = colors(cx);
    let fetching = matches!(state, Download::Fetching(_));
    let downloaded = matches!(state, Download::Downloaded(_));
    let lit_tile = over_tile && fetching;

    let (icon, icon_size) = match &state {
        Download::Remote | Download::Failed(_) => ("download", ControlSize::Large.symbol()),
        Download::Fetching(_) => ("x", ControlSize::Small.symbol()),
        Download::Downloaded(_) => (file.symbol(), ControlSize::Large.symbol()),
    };
    let detail = match &state {
        Download::Fetching(fraction) => {
            format!("{} / {}", file_size((fraction * file.bytes as f64) as u64), file_size(file.bytes))
        }
        Download::Failed(reason) => first_sentence(reason),
        _ => file.detail(),
    };
    let tip = match &state {
        Download::Failed(reason) => Some(reason.clone()),
        _ => (!file.alt.is_empty()).then(|| file.alt.clone()),
    };
    let border = 1.;
    let text_left = INSET + TILE_SIDE + INSET - 4. - border;

    let tile =
        div()
            .id(tile_id)
            .absolute()
            .left(px(INSET - border))
            .top(px(INSET - border))
            .size(px(TILE_SIDE))
            .rounded(px(Radius::MD))
            .bg(if lit_tile { c.background_secondary_accent_stronger } else { c.background_secondary_accent })
            .when_some(ring, |tile, fraction| {
                tile.child(div().absolute().inset(px(RING_INSET)).child(progress_ring(
                    fraction as f32,
                    c.muted_strongest_foreground,
                    c.foreground,
                )))
            })
            .child(div().absolute().inset_0().flex().items_center().justify_center().child(
                icons::symbol(icon, icon_size).text_color(if lit_tile { c.foreground } else { c.muted_foreground }),
            ))
            .on_hover(crate::ui::track_hover(&tile_hover))
            .when(fetching, |tile| {
                let (store, id) = (store.clone(), file.id.clone());
                tile.on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    FileDownloads::cancel(&store, &id, cx);
                })
            });

    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(
            crate::ui::middle_truncated(file.name.clone())
                .h(px(LINE_HEIGHT))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.foreground),
        )
        .child(
            div()
                .h(px(LINE_HEIGHT))
                .text_size(px(12.))
                .font_features(crate::ui::tabular_numbers())
                .text_color(if matches!(state, Download::Failed(_)) { c.destructive } else { c.muted_foreground })
                .truncate()
                .child(detail),
        );

    let reveal = match &state {
        Download::Downloaded(downloaded) => {
            let revealed = downloaded.clone();
            Some(
                div()
                    .id(reveal_id)
                    .ml(px(INSET))
                    .mr(px(BUTTON_MARGIN - border))
                    .on_hover(crate::ui::track_hover(&reveal_hover))
                    .child(
                        RowButton::new(
                            SharedString::from(format!("reveal-{}", model.row.id)),
                            REVEAL_TITLE,
                            REVEAL_TITLE,
                        )
                        .bordered()
                        .hover(c.background_secondary_accent)
                        .on_click(move |_, _, cx| cx.reveal_path(&revealed)),
                    ),
            )
        }
        _ => None,
    };
    let content = div()
        .absolute()
        .left(px(text_left))
        .top_0()
        .bottom_0()
        .right_0()
        .when(!downloaded, |content| content.pr(px(INSET - border)))
        .flex()
        .items_center()
        .child(text)
        .children(reveal);

    let pressed = {
        let (store, file, state) = (store.clone(), file.clone(), state.clone());
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| match &state {
            Download::Remote | Download::Failed(_) => FileDownloads::download(&store, &file, cx),
            Download::Fetching(_) => {}
            Download::Downloaded(downloaded) => cx.open_with_system(downloaded),
        }
    };
    let (menu_store, menu_file) = (store.clone(), file.clone());
    let recheck_id = file.id.clone();
    let card_hovered = card_hover.clone();
    div()
        .py(px(GAP))
        .child(
            div()
                .id(card_id)
                .relative()
                .w_full()
                .max_w(px(WIDEST))
                .h(px(CARD_HEIGHT))
                .rounded(px(Radius::LG))
                .border_1()
                .border_color(c.border)
                .bg(if over_card && !over_reveal && !fetching {
                    c.background_secondary_accent
                } else {
                    c.background_secondary
                })
                .cursor_pointer()
                .child(content)
                .child(tile)
                .when_some(tip, |card, tip| card.tooltip(crate::ui::tooltip(tip)))
                .on_hover(move |hovered, window, cx| {
                    if *hovered && !*card_hovered.read(cx) {
                        FileDownloads::recheck(&recheck_id, cx);
                    }
                    crate::ui::track_hover(&card_hovered)(hovered, window, cx);
                })
                .on_click(pressed)
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    let state = FileDownloads::state(&menu_store, &menu_file.id, cx);
                    menu(&menu_store, &menu_file, &state).show(event.position, window, cx);
                }),
        )
        .into_any_element()
}
