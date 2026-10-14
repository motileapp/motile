//! The images and videos of a message or of the composer, one at a time over the whole window.
//! ← and → go through them, and Esc or a click beside the picture closes it. An image zooms as
//! a `ZoomableImage` does. ⌘C copies what is shown and ⌘S saves it.

use std::path::PathBuf;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::MediaFiles;
use super::zoom::ZoomableImage;
use crate::models::{MediaSource, ViewedMedia};
use crate::store::Store;
use crate::theme::{ControlSize, Radius, colors};
use crate::ui::parts::CopyButton;
use crate::ui::{ActionButton, Spinner, Variant};

/// The room the picture leaves around it when it fits the window.
pub const MARGIN: (f32, f32) = (64., 52.);

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
    _subscription: Subscription,
}

impl MediaViewer {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let subscription = cx.observe(&store, |this, _, cx| this.follow_store(cx));
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
            _subscription: subscription,
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
            let viewer = cx.entity().downgrade();
            let image = ZoomableImage::new(&file, size(MARGIN.0, MARGIN.1), Radius::MD)
                .over_picture()
                .on_clicked_beside(move |_, cx| {
                    let _ = viewer.update(cx, |viewer, cx| viewer.close(cx));
                });
            self.image = Some(cx.new(|_| image));
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
        match keystroke.key.as_str() {
            "c" if command && self.file.is_some() => {
                self.copy(cx);
                self.copies += 1;
                cx.notify();
            }
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
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
            ActionButton::icon(id, symbol, help)
                .overlay()
                .large()
                .round(true)
                .symbol_size(ControlSize::Large.small_symbol())
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
            .text_size(px(13.))
            .text_color(c.muted_foreground_over_picture)
            .child(words)
    }
}

impl Render for MediaViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = crate::window_frame::content_size(window);
        let Some(viewing) = self.store.read(cx).viewing.clone() else { return div().into_any_element() };
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
            .text_size(px(13.))
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
        let corner = div()
            .absolute()
            .top(px(crate::theme::TOP_BAR + 10.))
            .right(px(10.))
            .flex()
            .gap(px(8.))
            .when(self.file.is_some(), |corner| {
                let copy = cx.entity().downgrade();
                corner
                    .child(
                        div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(
                            CopyButton::new(
                                "copy-viewed",
                                format!("Copy {} ({copy_keys})", if item.video { "Video" } else { "Image" }),
                                move |_, cx| {
                                    let _ = copy.update(cx, |viewer, cx| viewer.copy(cx));
                                },
                            )
                            .variant(Variant::Overlay)
                            .size(ControlSize::Large)
                            .symbol_size(ControlSize::Large.small_symbol())
                            .round(true)
                            .copies(self.copies),
                        ),
                    )
                    .child(Self::round_button(
                        "save-viewed",
                        "download",
                        format!("Save As ({save_keys})"),
                        cx.listener(|this, _, _, cx| this.save(cx)),
                    ))
            })
            .child(Self::round_button(
                "close-viewer",
                "x",
                "Close (Esc)",
                cx.listener(|this, _, _, cx| this.close(cx)),
            ));

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
                    .when(many && !full_screen, |viewer| {
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
                                    cx.listener(|this, _, _, cx| this.step(-1, cx)),
                                ))
                                .child(Self::round_button(
                                    "next",
                                    "chevron-right",
                                    "Next (→)",
                                    cx.listener(|this, _, _, cx| this.step(1, cx)),
                                )),
                        )
                    })
                    .when(!full_screen, |viewer| {
                        viewer
                            .child(
                                div()
                                    .absolute()
                                    .top(px(crate::theme::TOP_BAR + 14.))
                                    .left(px(80.))
                                    .right(px(80.))
                                    .flex()
                                    .justify_center()
                                    .child(title),
                            )
                            .child(corner)
                    }),
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
    fn video(&self, full_screen: bool, cx: &App) -> Option<AnyElement> {
        let player = self.player.clone()?;
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
    fn video(&self, _: bool, _: &App) -> Option<AnyElement> {
        None
    }
}
