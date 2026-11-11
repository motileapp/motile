//! The images and videos of a message or of the composer, one at a time over the whole window.
//! ← and → go through them, and Esc or a click beside the picture closes it. An image zooms as
//! a `ZoomableImage` does. ⌘C copies what is shown and ⌘S saves it, as a right click on it offers.
//! Its buttons sit in the window's top bar. An image can be marked up, to put in the composer.

use std::path::{Path, PathBuf};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::markup::{self, MarkupSession, MarkupTool, ink_color, ink_contrast};
use super::zoom::ZoomableImage;
use super::{MediaFiles, SAVE_TITLE};
use crate::models::{MediaSource, SavedMarkup, ViewedMedia};
use crate::store::Store;
use crate::theme::{ControlSize, Opacity, Radius, ShadowSize, Surface, TextSize, colors};
use crate::ui::menu::Menu;
use crate::ui::modal::{Answer, Confirmation, confirmation, presented};
use crate::ui::parts::CopyButton;
use crate::ui::{ActionButton, InputField, InputVariant, Spinner, Swatch, Variant, beside, tabular_numbers};

/// The room the picture leaves around it when it fits the window.
pub const MARGIN: (f32, f32) = (64., 52.);
/// The room between the corner's buttons' lights, which their clicks share.
const CORNER_GAP: f32 = 8.;
/// The room between the corner's buttons and the window's top and trailing edges.
const CORNER_INSET: f32 = 16.;
/// A small image is enlarged to fit the window, up to twice its size.
const LARGEST_FIT: f32 = 2.;
/// The room on each side of the name beside Cancel and Add to Message.
const MARKUP_NAME_INSET: f32 = 300.;
/// The room around the markup bar's controls, inside its capsule.
const BAR_INSET: f32 = 6.;
const BAR_SIZE: ControlSize = ControlSize::Large;
const NOTE_WIDTH: f32 = 340.;
/// The room around a picture being marked up, which keeps it clear of the bar under it: the
/// bar's place, its height with its inset and border, and a gap.
const MARKUP_MARGIN: (f32, f32) = (MARGIN.0, CORNER_INSET + 8. + BAR_SIZE.height() + 2. * (BAR_INSET + 1.) + 12.);

pub struct MediaViewer {
    store: Entity<Store>,
    focus: FocusHandle,
    shown: Option<ViewedMedia>,
    /// The file of what is shown, once it is on this device.
    file: Option<PathBuf>,
    failed: bool,
    #[cfg(target_os = "macos")]
    player: Option<Entity<super::video::VideoPlayer>>,
    /// The image shown, which zooms.
    image: Option<Entity<ZoomableImage>>,
    /// How many times ⌘C copied, for the copy button's check mark.
    copies: usize,
    /// A press fell on the darkness around what is shown, which closes it once let go.
    pressed_beside: bool,
    /// The image being marked up.
    markup: Option<Entity<MarkupSession>>,
    /// The marks a composer's attachment was given, to go on with once the image before them shows.
    resuming: Option<SavedMarkup>,
    /// The selected pin's note, and the pin it is of.
    note: Entity<InputState>,
    noted: Option<usize>,
    /// Asks whether to throw the marks away.
    discarding: bool,
    _markup_changes: Option<Subscription>,
    _subscriptions: [Subscription; 2],
}

impl MediaViewer {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let subscription = cx.observe(&store, |this, _, cx| this.follow_store(cx));
        let note = cx.new(|cx| InputState::new(window, cx).placeholder("Note for the agent"));
        let writing = cx.subscribe_in(&note, window, |this, note, event: &InputEvent, _, cx| {
            let Some(markup) = this.markup.clone() else { return };
            match event {
                InputEvent::Change => {
                    let text = note.read(cx).value().to_string();
                    markup.update(cx, |markup, _| markup.note = text);
                }
                InputEvent::PressEnter { .. } => this.change_markup(cx, MarkupSession::deselect),
                _ => {}
            }
        });
        let mut viewer = Self {
            store,
            focus,
            shown: None,
            file: None,
            failed: false,
            #[cfg(target_os = "macos")]
            player: None,
            image: None,
            copies: 0,
            pressed_beside: false,
            markup: None,
            resuming: None,
            note,
            noted: None,
            discarding: false,
            _markup_changes: None,
            _subscriptions: [subscription, writing],
        };
        viewer.follow_store(cx);
        viewer
    }

    fn follow_store(&mut self, cx: &mut Context<Self>) {
        let item = self.store.read(cx).viewing.as_ref().map(|viewing| viewing.item().clone());
        if item == self.shown {
            return;
        }
        self.pause(cx);
        self.end_markup(cx);
        self.shown = item.clone();
        self.file = None;
        self.failed = false;
        #[cfg(target_os = "macos")]
        {
            self.player = None;
        }
        self.image = None;
        let Some(item) = item else { return };
        match &item.source {
            MediaSource::File(file) => self.show(item.clone(), Ok(file.clone()), cx),
            MediaSource::Media(id) => {
                let view = cx.entity().downgrade();
                let wanted = item.clone();
                MediaFiles::fetch(&self.store, id, cx, move |file, cx| {
                    let _ = view.update(cx, |viewer, cx| viewer.show(wanted, file, cx));
                });
            }
        }
        cx.notify();
    }

    fn show(&mut self, item: ViewedMedia, file: Result<PathBuf, String>, cx: &mut Context<Self>) {
        if self.shown.as_ref() != Some(&item) {
            return;
        }
        let Ok(file) = file else {
            self.failed = true;
            return cx.notify();
        };
        self.file = Some(file.clone());
        if !item.video {
            self.image = Some(self.zoomable(&file, cx));
            return cx.notify();
        }
        // A video the system's player can't play opens in the app the system has for it.
        if !super::plays_here(&file) {
            cx.open_with_system(&file);
            return self.close(cx);
        }
        #[cfg(target_os = "macos")]
        {
            self.player = Some(cx.new(|cx| super::video::VideoPlayer::new(&file, cx)));
        }
        #[cfg(not(target_os = "macos"))]
        cx.open_with_system(&file);
        cx.notify();
    }

    fn zoomable(&self, file: &Path, cx: &mut Context<Self>) -> Entity<ZoomableImage> {
        let (viewer, menu) = (cx.entity().downgrade(), cx.entity().downgrade());
        let image = ZoomableImage::new(file, size(MARGIN.0, MARGIN.1), Radius::MD)
            .largest_fit(LARGEST_FIT)
            .over_picture()
            .on_clicked_beside(move |_, cx| {
                let _ = viewer.update(cx, |viewer, cx| {
                    if viewer.markup.is_none() {
                        viewer.close(cx);
                    }
                });
            })
            .on_right_click(move |position, window, cx| {
                let Ok(menu) = menu.update(cx, |viewer, cx| viewer.menu(cx)) else { return };
                menu.show(position, window, cx);
            });
        cx.new(|_| image)
    }

    /// An image that is here can be marked up.
    fn markable(&self, cx: &mut App) -> bool {
        let Some(image) = &self.image else { return false };
        self.file.is_some() && ZoomableImage::picture_size(image, cx).is_some()
    }

    /// Marks up the image shown, or goes on with the marks it was given in the composer, over the
    /// image before them.
    fn start_markup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.markable(cx) {
            return;
        }
        let (Some(item), Some(file), Some(image)) = (self.shown.clone(), self.file.clone(), self.image.clone()) else {
            return;
        };
        let saved = item.attachment.as_ref().and_then(|id| {
            self.store.read(cx).attachments().into_iter().find(|attachment| &attachment.id == id)?.markup
        });
        let Some(saved) = saved else {
            let Some(pixels) = ZoomableImage::picture_size(&image, cx) else { return };
            let opened = MarkupSession::open(file, item.name, item.attachment, pixels, None);
            return self.begin_markup(opened, window, cx);
        };
        self.image = Some(self.zoomable(&saved.original, cx));
        self.resuming = Some(saved);
        self.resume_markup(window, cx);
    }

    /// Goes on with the marks of a composer's attachment once the image before them shows.
    fn resume_markup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(saved), Some(image), Some(item)) = (self.resuming.clone(), self.image.clone(), self.shown.clone())
        else {
            return;
        };
        let Some(pixels) = ZoomableImage::picture_size(&image, cx) else { return };
        self.resuming = None;
        let opened =
            MarkupSession::open(saved.original.clone(), saved.name.clone(), item.attachment, pixels, Some(&saved));
        self.begin_markup(opened, window, cx);
    }

    fn begin_markup(&mut self, opened: Option<MarkupSession>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(opened) = opened else { return };
        let markup = cx.new(|_| opened);
        self._markup_changes = Some(cx.observe_in(&markup, window, |this, _, window, cx| this.follow_note(window, cx)));
        if let Some(image) = &self.image {
            image.update(cx, |image, cx| {
                image.set_markup(Some(markup.clone()), cx);
                image.set_margin(size(MARKUP_MARGIN.0, MARKUP_MARGIN.1), cx);
            });
        }
        self.markup = Some(markup);
        cx.notify();
    }

    /// Writes the selected pin's note in the field and gives it the keys, or gives them back.
    fn follow_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.notify();
        let Some(markup) = self.markup.clone() else { return };
        let selected = markup.read(cx).selected;
        if selected == self.noted {
            return;
        }
        self.noted = selected;
        if selected.is_none() {
            return self.focus.focus(window, cx);
        }
        let note = markup.read(cx).note.clone();
        self.note.update(cx, |field, cx| {
            field.set_value(note, window, cx);
            field.focus_handle(cx).focus(window, cx);
        });
    }

    fn change_markup(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut MarkupSession)) {
        let Some(markup) = &self.markup else { return };
        markup.update(cx, |markup, cx| {
            change(markup);
            cx.notify();
        });
    }

    fn end_markup(&mut self, cx: &mut Context<Self>) {
        let Some(markup) = self.markup.take() else {
            self.resuming = None;
            return;
        };
        let reload = Some(&markup.read(cx).source) != self.file.as_ref();
        (self.resuming, self.noted, self.discarding, self._markup_changes) = (None, None, false, None);
        match (reload, self.file.clone()) {
            (true, Some(file)) => self.image = Some(self.zoomable(&file, cx)),
            _ => {
                if let Some(image) = &self.image {
                    image.update(cx, |image, cx| {
                        image.set_markup(None, cx);
                        image.set_margin(size(MARGIN.0, MARGIN.1), cx);
                    });
                }
            }
        }
        cx.notify();
    }

    fn cancel_markup(&mut self, cx: &mut Context<Self>) {
        let Some(markup) = &self.markup else { return };
        if markup.read(cx).changed {
            self.discarding = true;
            return cx.notify();
        }
        self.end_markup(cx);
    }

    fn finish_markup(&mut self, cx: &mut Context<Self>) {
        let Some(markup) = self.markup.clone() else { return };
        let session = markup.read(cx);
        if session.finishing || !(session.changed || session.attachment.is_none()) {
            return;
        }
        let writing = markup.update(cx, |markup, cx| {
            cx.notify();
            markup.finish(markup::folder())
        });
        let written = cx.background_spawn(async move { writing() });
        cx.spawn(async move |this, cx| {
            let result = written.await;
            let _ = this.update(cx, |viewer, cx| {
                if viewer.markup.as_ref() != Some(&markup) {
                    return;
                }
                let replacing = markup.update(cx, |markup, cx| {
                    markup.finishing = false;
                    markup.failed = result.is_none();
                    cx.notify();
                    markup.attachment.clone()
                });
                let Some(result) = result else { return };
                viewer.end_markup(cx);
                viewer.store.update(cx, |store, cx| {
                    store.attach_markup(result, replacing.as_deref());
                    cx.notify();
                });
                viewer.close(cx);
            });
        })
        .detach();
    }

    /// While an image is marked up, the keys pick its tools and colours. The note's field keeps
    /// the keys it takes while a pin is selected.
    fn markup_key(&mut self, markup: Entity<MarkupSession>, keystroke: &Keystroke, cx: &mut Context<Self>) {
        if self.discarding {
            return;
        }
        let selected = markup.read(cx).selected.is_some();
        let key = keystroke.key.as_str();
        if key == "escape" {
            match selected {
                true => self.change_markup(cx, MarkupSession::deselect),
                false => self.cancel_markup(cx),
            }
            return cx.stop_propagation();
        }
        if selected {
            return;
        }
        if keystroke.modifiers.secondary() {
            match key {
                "z" if keystroke.modifiers.shift => self.change_markup(cx, MarkupSession::redo),
                "z" => self.change_markup(cx, MarkupSession::undo),
                _ => {
                    let Some(image) = self.image.clone() else { return };
                    if !image.update(cx, |image, cx| image.zoom_key(keystroke, cx)) {
                        return;
                    }
                }
            }
            return cx.stop_propagation();
        }
        let inks = markup.read(cx).inks.clone();
        if key == "enter" {
            self.finish_markup(cx);
        } else if let Some(tool) = MarkupTool::ALL.into_iter().find(|tool| tool.key().eq_ignore_ascii_case(key)) {
            self.change_markup(cx, |markup| markup.set_tool(tool));
        } else if let Some(ink) = key.parse::<usize>().ok().and_then(|digit| inks.get(digit.checked_sub(1)?)) {
            self.change_markup(cx, |markup| markup.set_ink(ink));
        }
        cx.stop_propagation();
    }

    fn pause(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        if let Some(player) = &self.player {
            player.update(cx, |player, _| player.pause());
        }
        let _ = cx;
    }

    /// Whether the video fills the screen, which then takes the keys.
    fn full_screen(&self, cx: &App) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(player) = &self.player {
            return player.read(cx).full_screen();
        }
        let _ = cx;
        false
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.pause(cx);
        self.store.update(cx, |store, cx| {
            store.close_viewer();
            cx.notify();
        });
    }

    fn step(&mut self, step: isize, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.view_next(step);
            cx.notify();
        });
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let (Some(file), Some(item)) = (self.file.clone(), self.shown.clone()) else { return };
        super::copy(file, item.video, item.name, cx);
    }

    /// Copies as ⌘C does, with the copy button's check mark.
    fn copy_checked(&mut self, cx: &mut Context<Self>) {
        self.copy(cx);
        self.copies += 1;
        cx.notify();
    }

    /// What a right click on the picture offers, as the corner's buttons do.
    fn menu(&self, cx: &mut Context<Self>) -> Menu {
        let video = self.shown.as_ref().is_some_and(|item| item.video);
        let (copy, save) = (cx.entity().downgrade(), cx.entity().downgrade());
        Menu::new()
            .symbol_item(copy_title(video), "copy", move |_, cx| {
                let _ = copy.update(cx, |viewer, cx| viewer.copy_checked(cx));
            })
            .symbol_item(SAVE_TITLE, "download", move |_, cx| {
                let _ = save.update(cx, |viewer, cx| viewer.save(cx));
            })
    }

    fn save(&self, cx: &mut App) {
        let (Some(file), Some(item)) = (self.file.clone(), self.shown.as_ref()) else { return };
        super::native::save(file, &item.name, cx);
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let command = keystroke.modifiers.secondary();
        // The video that fills the screen takes its own keys, as the system's player does.
        #[cfg(not(target_os = "macos"))]
        let _ = window;
        if self.full_screen(cx) {
            #[cfg(target_os = "macos")]
            if let Some(player) = self.player.clone() {
                let key = keystroke.key.as_str();
                let plain = !keystroke.modifiers.modified();
                let taken = plain && (key == "escape" || player.update(cx, |player, cx| player.key(key, cx)));
                if !taken {
                    return;
                }
                if key == "escape" {
                    player.update(cx, |player, cx| player.set_full_screen(false, window, cx));
                }
            }
            return cx.stop_propagation();
        }
        if let Some(markup) = self.markup.clone() {
            return self.markup_key(markup, keystroke, cx);
        }
        if keystroke.key == "m" && !keystroke.modifiers.modified() && self.markable(cx) {
            self.start_markup(window, cx);
            return cx.stop_propagation();
        }
        match keystroke.key.as_str() {
            "c" if command && self.file.is_some() => self.copy_checked(cx),
            "s" if command && self.file.is_some() => self.save(cx),
            "escape" => self.close(cx),
            "left" => self.step(-1, cx),
            "right" => self.step(1, cx),
            "space" =>
            {
                #[cfg(target_os = "macos")]
                if let Some(player) = &self.player {
                    player.update(cx, |player, cx| player.toggle(cx));
                }
            }
            _ if command => {
                let Some(image) = self.image.clone() else { return };
                if !image.update(cx, |image, cx| image.zoom_key(keystroke, cx)) {
                    return;
                }
            }
            _ => {}
        }
        cx.stop_propagation();
    }

    /// A round button over the picture, as a control is in the dark.
    fn round_button(
        id: &'static str,
        symbol: &'static str,
        help: impl Into<SharedString>,
        margin: Edges<f32>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
            ActionButton::icon(id, symbol, help)
                .overlay()
                .large()
                .round(true)
                .symbol_size(ControlSize::Large.small_symbol())
                .margin(margin)
                .on_click(on_click),
        )
    }

    fn status(&self, item: &ViewedMedia, progress: Option<f64>, failed: bool, cx: &App) -> Div {
        let c = colors(cx);
        let words: AnyElement = if failed {
            div()
                .child(if item.video { "This video couldn't be loaded." } else { "This image couldn't be loaded." })
                .into_any_element()
        } else if let Some(fraction) = progress {
            div()
                .font_features(crate::ui::tabular_numbers())
                .child(format!("Downloading {} · {}%", item.name, (fraction * 100.) as u32))
                .into_any_element()
        } else {
            Spinner::new(ControlSize::Large.symbol()).render(cx).into_any_element()
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .text_size(TextSize::Sm)
            .text_color(c.muted_foreground_over_picture)
            .child(words)
    }
}

impl Render for MediaViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = crate::window_frame::content_size(window);
        let Some(viewing) = self.store.read(cx).viewing.clone() else { return div().into_any_element() };
        self.resume_markup(window, cx);
        let markup = self.markup.clone();
        let marking = markup.is_some();
        let markable = self.markable(cx);
        let item = viewing.item().clone();
        let progress = match &item.source {
            MediaSource::Media(id) => self.store.read(cx).media_progress.get(id).copied(),
            MediaSource::File(_) => None,
        };
        let many = viewing.items.len() > 1;
        let full_screen = self.full_screen(cx);
        let c = colors(cx);

        let content: AnyElement = if let Some(image) = self.image.clone() {
            div().absolute().inset_0().child(image).into_any_element()
        } else {
            match self.video(full_screen, cx) {
                Some(video) => video,
                None => self.status(&item, progress, self.failed, cx).into_any_element(),
            }
        };

        let title = div()
            .h(px(24.))
            .px(px(10.))
            .rounded_full()
            .bg(c.scrim)
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(TextSize::Sm)
            .font_weight(FontWeight::MEDIUM)
            .text_color(c.foreground_over_picture)
            .child(crate::ui::middle_truncated(item.name.clone()))
            .when(many, |title| {
                title.child(div().flex_none().text_color(c.muted_foreground_over_picture).child(format!(
                    "{} of {}",
                    viewing.index + 1,
                    viewing.items.len()
                )))
            });

        let shortcuts = &self.store.read(cx).shortcuts;
        let (copy_keys, save_keys) = (shortcuts.keys_label("mod+c"), shortcuts.keys_label("mod+s"));
        let trailing = CORNER_INSET + crate::window_frame::trailing_room(window, cx);
        let buttons = 4. * ControlSize::Large.height() + 3. * CORNER_GAP;
        let name_inset = match marking {
            true => MARKUP_NAME_INSET,
            false => (trailing + buttons).max(crate::window_frame::leading_room(window, cx)) + CORNER_INSET,
        };
        let corner = div()
            .absolute()
            .top_0()
            .right(px(trailing))
            .flex()
            .items_center()
            .children(markup.as_ref().map(|markup| self.markup_buttons(markup, cx)))
            .when(!marking && markable, |corner| {
                corner.child(Self::round_button(
                    "mark-up",
                    "pencil.line",
                    "Mark Up (M)",
                    beside(0., CORNER_GAP / 2.),
                    cx.listener(|this, _, window, cx| this.start_markup(window, cx)),
                ))
            })
            .when(!marking && self.file.is_some(), |corner| {
                let copy = cx.entity().downgrade();
                corner
                    .child(
                        div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
                            CopyButton::new(
                                "copy-viewed",
                                format!("{} ({copy_keys})", copy_title(item.video)),
                                move |_, cx| {
                                    let _ = copy.update(cx, |viewer, cx| viewer.copy(cx));
                                },
                            )
                            .variant(Variant::Overlay)
                            .size(ControlSize::Large)
                            .symbol_size(ControlSize::Large.small_symbol())
                            .round(true)
                            .copies(self.copies)
                            .margin(beside(if markable { CORNER_GAP / 2. } else { 0. }, CORNER_GAP / 2.)),
                        ),
                    )
                    .child(Self::round_button(
                        "save-viewed",
                        "download",
                        format!("Save As ({save_keys})"),
                        beside(CORNER_GAP / 2., CORNER_GAP / 2.),
                        cx.listener(|this, _, _, cx| this.save(cx)),
                    ))
            })
            .when(!marking, |corner| {
                corner.child(Self::round_button(
                    "close-viewer",
                    "x",
                    "Close (Esc)",
                    beside(if self.file.is_some() { CORNER_GAP / 2. } else { 0. }, 0.),
                    cx.listener(|this, _, _, cx| this.close(cx)),
                ))
            });
        let bar = markup.as_ref().map(|markup| self.markup_bar(markup, cx));
        let discarding = self.discard_alert(window, cx);

        deferred(
            anchored().position(crate::window_frame::content_origin(window)).child(
                div()
                    .id("media-viewer")
                    .track_focus(&self.focus)
                    .w(viewport.width)
                    .h(viewport.height)
                    .relative()
                    .bg(if full_screen { c.overlay } else { c.scrim })
                    .occlude()
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| this.key(event, window, cx)))
                    // A click on the darkness closes it; a video takes its own clicks, and an image
                    // says itself when one falls beside it.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.pressed_beside = this.image.is_none() && !this.full_screen(cx);
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if std::mem::take(&mut this.pressed_beside) {
                                this.close(cx);
                            }
                        }),
                    )
                    .child(content)
                    .when(many && !full_screen && !marking, |viewer| {
                        viewer.child(
                            div()
                                .absolute()
                                .left(px(14.))
                                .right(px(14.))
                                .top_0()
                                .bottom_0()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(Self::round_button(
                                    "previous",
                                    "chevron-left",
                                    "Previous (←)",
                                    Edges::default(),
                                    cx.listener(|this, _, _, cx| this.step(-1, cx)),
                                ))
                                .child(Self::round_button(
                                    "next",
                                    "chevron-right",
                                    "Next (→)",
                                    Edges::default(),
                                    cx.listener(|this, _, _, cx| this.step(1, cx)),
                                )),
                        )
                    })
                    // One row in the top bar places the name and the buttons, so they share a
                    // middle. The name is centered on the window, as far from both ends as all
                    // four buttons need, so it keeps clear of them however many show.
                    .when(!full_screen, |viewer| {
                        viewer.child(
                            div()
                                .absolute()
                                .top(px(CORNER_INSET))
                                .left_0()
                                .right_0()
                                .h(px(ControlSize::Large.height()))
                                .px(px(name_inset))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(title)
                                .child(corner),
                        )
                    })
                    .children(bar)
                    .children(discarding),
            ),
        )
        .with_priority(2)
        .into_any_element()
    }
}

impl MediaViewer {
    /// The video in its place, with a spinner until its first frame is there, or filling the
    /// window while it is full screen.
    #[cfg(target_os = "macos")]
    fn video(&self, full_screen: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let player = self.player.clone()?;
        let menu = cx.entity().downgrade();
        let ready = player.read(cx).ready();
        let c = colors(cx);
        Some(
            div()
                .absolute()
                .when(full_screen, |video| video.inset_0())
                .when(!full_screen, |video| {
                    video
                        .left(px(MARGIN.0))
                        .right(px(MARGIN.0))
                        .top(px(MARGIN.1))
                        .bottom(px(MARGIN.1))
                        .rounded(px(Radius::MD))
                        .overflow_hidden()
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when(!full_screen, |video| {
                    video.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        let Ok(menu) = menu.update(cx, |viewer, cx| viewer.menu(cx)) else { return };
                        cx.stop_propagation();
                        menu.show(event.position, window, cx);
                    })
                })
                .child(player)
                .when(!ready, |video| {
                    video.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(c.muted_foreground_over_picture)
                            .child(Spinner::new(ControlSize::Large.symbol()).render(cx)),
                    )
                })
                .into_any_element(),
        )
    }

    #[cfg(not(target_os = "macos"))]
    fn video(&self, _: bool, _: &mut Context<Self>) -> Option<AnyElement> {
        None
    }
}

impl MediaViewer {
    /// Cancel, and Add to Message, or Done for an image that is already in the composer.
    fn markup_buttons(&self, markup: &Entity<MarkupSession>, cx: &mut Context<Self>) -> Div {
        let markup = markup.read(cx);
        let title = if markup.attachment.is_none() { "Add to Message" } else { "Done" };
        div()
            .flex()
            .items_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                ActionButton::new("cancel-markup", "Cancel")
                    .help("Cancel (Esc)")
                    .overlay()
                    .large()
                    .round(true)
                    .margin(beside(0., CORNER_GAP / 2.))
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_markup(cx))),
            )
            .child(
                ActionButton::new("finish-markup", title)
                    .help("Return")
                    .primary()
                    .large()
                    .round(true)
                    .pending(markup.finishing)
                    .disabled(!markup.changed && markup.attachment.is_some())
                    .margin(beside(CORNER_GAP / 2., 0.))
                    .on_click(cx.listener(|this, _, _, cx| this.finish_markup(cx))),
            )
    }

    /// The bar under an image being marked up: the tools, the colours, undo and redo, and what
    /// went wrong over it. While a pin is selected it is the pin's note instead, which Return keeps.
    fn markup_bar(&self, markup: &Entity<MarkupSession>, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let session = markup.read(cx);
        let failed = session.failed;
        let controls = match session.selected_pin().cloned() {
            Some(pin) => self.note_bar(&pin, cx),
            None => self.tool_bar(markup, cx),
        };
        let bar = div()
            .flex()
            .items_center()
            .p(px(BAR_INSET))
            .rounded_full()
            .bg(c.popover)
            .border_1()
            .border_color(c.border_popover)
            .shadow(c.shadow(ShadowSize::Lg, Opacity::Shadow))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(controls);
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(CORNER_INSET + 8.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .when(failed, |column| {
                column.child(
                    div()
                        .h(px(24.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .rounded_full()
                        .bg(c.scrim)
                        .text_size(TextSize::Sm)
                        .text_color(c.foreground_over_picture)
                        .child("This image couldn't be marked up."),
                )
            })
            .child(bar)
    }

    fn tool_bar(&self, markup: &Entity<MarkupSession>, cx: &mut Context<Self>) -> Div {
        let session = markup.read(cx);
        let c = colors(cx);
        let shortcuts = &self.store.read(cx).shortcuts;
        let (undo_keys, redo_keys) = (shortcuts.keys_label("mod+z"), shortcuts.keys_label("mod+shift+z"));
        let tools = MarkupTool::ALL.into_iter().enumerate().map(|(index, tool)| {
            ActionButton::icon(("markup-tool", index), tool.symbol(), format!("{} ({})", tool.title(), tool.key()))
                .size(BAR_SIZE)
                .round(true)
                .surface(Surface::Popover)
                .selected(session.tool == tool)
                .on_click(cx.listener(move |this, _, _, cx| this.change_markup(cx, |markup| markup.set_tool(tool))))
        });
        let inks = session.inks.iter().enumerate().map(|(index, ink)| {
            let picked = ink.clone();
            Swatch::new(("markup-ink", index), ink_color(ink, c), format!("{} ({})", capitalized(ink), index + 1))
                .size(BAR_SIZE)
                .selected(session.ink == *ink)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let picked = picked.clone();
                    this.change_markup(cx, move |markup| markup.set_ink(&picked))
                }))
        });
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            .children(tools)
            .child(bar_divider(cx))
            .children(inks)
            .child(bar_divider(cx))
            .child(
                ActionButton::icon("markup-undo", "undo-2", format!("Undo ({undo_keys})"))
                    .size(BAR_SIZE)
                    .round(true)
                    .surface(Surface::Popover)
                    .disabled(!session.can_undo)
                    .on_click(cx.listener(|this, _, _, cx| this.change_markup(cx, MarkupSession::undo))),
            )
            .child(
                ActionButton::icon("markup-redo", "redo-2", format!("Redo ({redo_keys})"))
                    .size(BAR_SIZE)
                    .round(true)
                    .surface(Surface::Popover)
                    .disabled(!session.can_redo)
                    .on_click(cx.listener(|this, _, _, cx| this.change_markup(cx, MarkupSession::redo))),
            )
    }

    fn note_bar(&self, pin: &markup::Pin, cx: &mut Context<Self>) -> Div {
        let c = colors(cx);
        let side = px(BAR_SIZE.height());
        let badge = div()
            .flex_none()
            .size(side)
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(ink_color(&pin.ink, c))
            .text_color(ink_contrast(&pin.ink, c))
            .text_size(TextSize::Sm)
            .font_weight(FontWeight::SEMIBOLD)
            .font_features(tabular_numbers())
            .child(pin.number.to_string());
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(badge)
            .child(div().w(px(NOTE_WIDTH)).h(side).child(
                InputField::new(&self.note).variant(InputVariant::Bare).size(BAR_SIZE).surface(Surface::Popover),
            ))
            .child(
                ActionButton::icon("remove-pin", "trash-2", "Remove Pin")
                    .size(BAR_SIZE)
                    .round(true)
                    .surface(Surface::Popover)
                    .on_click(cx.listener(|this, _, _, cx| this.change_markup(cx, MarkupSession::remove_selected))),
            )
            .child(
                ActionButton::icon("keep-note", "check", "Keep Note (Return)")
                    .primary()
                    .size(BAR_SIZE)
                    .round(true)
                    .on_click(cx.listener(|this, _, _, cx| this.change_markup(cx, MarkupSession::deselect))),
            )
    }

    fn discard_alert(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (_, leaving) = presented("discard-marks", self.discarding.then_some(()), window, cx)?;
        let (keep, discard) = (cx.entity().downgrade(), cx.entity().downgrade());
        let asked = Confirmation::new("Discard your marks?").message("The image stays as it was.").answer(
            Answer::new("Discard", move |_, cx| {
                let _ = discard.update(cx, |viewer, cx| viewer.end_markup(cx));
            })
            .danger(),
        );
        let close = move |_: &mut Window, cx: &mut App| {
            let _ = keep.update(cx, |viewer, cx| {
                viewer.discarding = false;
                cx.notify();
            });
        };
        Some(confirmation("discard-marks", asked, close, cx).leaving(leaving).into_any_element())
    }
}

fn bar_divider(cx: &App) -> Div {
    div().flex_none().mx(px(4.)).w(px(1.)).h(px(BAR_SIZE.height() * 0.6)).bg(colors(cx).border_popover)
}

fn capitalized(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect()).unwrap_or_default()
}

fn copy_title(video: bool) -> &'static str {
    if video { "Copy Video" } else { "Copy Image" }
}
