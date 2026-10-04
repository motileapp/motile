//! The images and videos of a message or of the composer, one at a time over the whole window.
//! ← and → go through them, and Esc or a click beside the picture closes it. An image zooms as
//! it does in Preview: a double click, a pinch, or ⌘+, ⌘−, ⌘0 for its real size and ⌘9 to fit.

use std::path::PathBuf;

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::MediaFiles;
use crate::models::{MediaSource, ViewedMedia};
use crate::store::Store;
use crate::ui::icons;

/// The room the picture leaves around it when it fits the window.
pub const MARGIN: (f32, f32) = (64., 52.);
const MAX_ZOOM: f32 = 8.;

enum Loaded {
    Image {
        path: PathBuf,
        width: f32,
        height: f32,
    },
    #[cfg(target_os = "macos")]
    Video(Entity<super::video::VideoPlayer>),
}

pub struct MediaViewer {
    store: Entity<Store>,
    focus: FocusHandle,
    shown: Option<ViewedMedia>,
    loaded: Option<Loaded>,
    failed: bool,
    /// How much the image is magnified, and how far it is moved from the middle.
    scale: Option<f32>,
    offset: Point<Pixels>,
    dragging: Option<Point<Pixels>>,
    dragged: bool,
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
            loaded: None,
            failed: false,
            scale: None,
            offset: Point::default(),
            dragging: None,
            dragged: false,
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
        self.loaded = None;
        self.failed = false;
        self.scale = None;
        self.offset = Point::default();
        let Some(item) = item else { return };
        match &item.source {
            MediaSource::File(file) => self.show(item.clone(), Some(file.clone()), cx),
            MediaSource::Media(id) => {
                let view = cx.entity().downgrade();
                let wanted = item.clone();
                let path = MediaFiles::fetch_then(&self.store, id, cx, move |path, cx| {
                    let _ = view.update(cx, |viewer, cx| viewer.show(wanted, path, cx));
                });
                if path.is_some() {
                    self.show(item, path, cx);
                }
            }
        }
        cx.notify();
    }

    fn show(&mut self, item: ViewedMedia, file: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.shown.as_ref() != Some(&item) {
            return;
        }
        let Some(file) = file else {
            self.failed = true;
            return cx.notify();
        };
        if !item.video {
            match image::image_dimensions(&file) {
                Ok((width, height)) => {
                    self.loaded = Some(Loaded::Image { path: file, width: width as f32, height: height as f32 });
                }
                Err(_) => self.failed = true,
            }
            return cx.notify();
        }
        // A video the system's player can't play opens in the app the system has for it.
        if !super::plays_here(&file) {
            cx.open_with_system(&file);
            return self.close(cx);
        }
        #[cfg(target_os = "macos")]
        {
            self.loaded = Some(Loaded::Video(cx.new(|cx| super::video::VideoPlayer::new(&file, cx))));
        }
        #[cfg(not(target_os = "macos"))]
        cx.open_with_system(&file);
        cx.notify();
    }

    fn pause(&mut self, cx: &mut Context<Self>) {
        #[cfg(target_os = "macos")]
        if let Some(Loaded::Video(player)) = &self.loaded {
            player.update(cx, |player, _| player.pause());
        }
        let _ = cx;
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

    /// The scale at which the image fits the window, never larger than its real size.
    fn fit(&self, viewport: Size<Pixels>) -> f32 {
        let Some(Loaded::Image { width, height, .. }) = &self.loaded else { return 1. };
        let across = (f32::from(viewport.width) - 2. * MARGIN.0) / width;
        let down = (f32::from(viewport.height) - 2. * MARGIN.1) / height;
        across.min(down).clamp(0.01, 1.)
    }

    fn zoom_to(&mut self, scale: f32, around: Option<Point<Pixels>>, viewport: Size<Pixels>, cx: &mut Context<Self>) {
        let fit = self.fit(viewport);
        let old = self.scale.unwrap_or(fit);
        let new = scale.clamp(fit, MAX_ZOOM);
        // The point under the pointer stays where it is.
        if let Some(around) = around {
            let center = point(viewport.width / 2., viewport.height / 2.);
            let from_center = around - center - self.offset;
            self.offset -= from_center * (new / old - 1.);
        }
        self.scale = Some(new);
        if (new - fit).abs() < 0.001 {
            self.offset = Point::default();
        }
        self.clamp_offset(viewport);
        cx.notify();
    }

    fn clamp_offset(&mut self, viewport: Size<Pixels>) {
        let Some(Loaded::Image { width, height, .. }) = &self.loaded else { return };
        let scale = self.scale.unwrap_or(self.fit(viewport));
        let spare_x = ((width * scale - f32::from(viewport.width)) / 2.).max(0.);
        let spare_y = ((height * scale - f32::from(viewport.height)) / 2.).max(0.);
        self.offset.x = self.offset.x.clamp(px(-spare_x), px(spare_x));
        self.offset.y = self.offset.y.clamp(px(-spare_y), px(spare_y));
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        let keystroke = &event.keystroke;
        let command = keystroke.modifiers.platform;
        match keystroke.key.as_str() {
            "escape" => self.close(cx),
            "left" => self.step(-1, cx),
            "right" => self.step(1, cx),
            "space" =>
            {
                #[cfg(target_os = "macos")]
                if let Some(Loaded::Video(player)) = &self.loaded {
                    player.update(cx, |player, cx| player.toggle(cx));
                }
            }
            "=" | "+" if command => {
                let scale = self.scale.unwrap_or(self.fit(viewport));
                self.zoom_to(scale * 1.5, None, viewport, cx);
            }
            "-" if command => {
                let scale = self.scale.unwrap_or(self.fit(viewport));
                self.zoom_to(scale / 1.5, None, viewport, cx);
            }
            "0" if command => self.zoom_to(1., None, viewport, cx),
            "9" if command => self.zoom_to(self.fit(viewport), None, viewport, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn round_button(id: &'static str, symbol: &'static str, help: &'static str) -> Stateful<Div> {
        div()
            .id(id)
            .size(px(32.))
            .rounded_full()
            .bg(hsla(0., 0., 0.2, 0.75))
            .flex()
            .items_center()
            .justify_center()
            .child(icons::symbol(symbol, 13.).text_color(white()))
            .tooltip(crate::ui::tooltip(help))
    }
}

impl Render for MediaViewer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let Some(viewing) = self.store.read(cx).viewing.clone() else { return div().into_any_element() };
        let item = viewing.item().clone();
        let progress = match &item.source {
            MediaSource::Media(id) => self.store.read(cx).media_progress.get(id).copied(),
            MediaSource::File(_) => None,
        };
        let many = viewing.items.len() > 1;
        let fit = self.fit(viewport);
        let scale = self.scale.unwrap_or(fit);

        let content: AnyElement = match &self.loaded {
            Some(Loaded::Image { path, width, height }) => {
                let (shown_width, shown_height) = (width * scale, height * scale);
                let left = (viewport.width - px(shown_width)) / 2. + self.offset.x;
                let top = (viewport.height - px(shown_height)) / 2. + self.offset.y;
                div()
                    .id("viewed-image")
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(px(shown_width))
                    .h(px(shown_height))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .child(img(path.clone()).size_full().object_fit(ObjectFit::Fill))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            let viewport = window.viewport_size();
                            if event.click_count == 2 {
                                let fit = this.fit(viewport);
                                let zoomed = this.scale.unwrap_or(fit) > fit + 0.001;
                                let target = if zoomed { fit } else { (fit * 2.).max(1.) };
                                this.zoom_to(target, Some(event.position), viewport, cx);
                                return;
                            }
                            this.dragging = Some(event.position);
                            this.dragged = false;
                        }),
                    )
                    .into_any_element()
            }
            #[cfg(target_os = "macos")]
            Some(Loaded::Video(player)) => div()
                .absolute()
                .left(px(MARGIN.0))
                .right(px(MARGIN.0))
                .top(px(MARGIN.1))
                .bottom(px(MARGIN.1))
                .rounded(px(8.))
                .overflow_hidden()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(player.clone())
                .into_any_element(),
            None => {
                let words: AnyElement = if self.failed {
                    div()
                        .child(if item.video {
                            "This video couldn't be loaded."
                        } else {
                            "This image couldn't be loaded."
                        })
                        .into_any_element()
                } else if let Some(fraction) = progress {
                    div().child(format!("Downloading {} · {}%", item.name, (fraction * 100.) as u32)).into_any_element()
                } else {
                    crate::ui::spinner(14., cx).into_any_element()
                };
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.))
                    .text_color(hsla(0., 0., 1., 0.7))
                    .child(words)
                    .into_any_element()
            }
        };

        let title = div()
            .h(px(24.))
            .px(px(10.))
            .rounded_full()
            .bg(hsla(0., 0., 0., 0.5))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(white())
            .child(div().truncate().child(item.name.clone()))
            .when(many, |title| {
                title.child(div().text_color(hsla(0., 0., 1., 0.6)).child(format!(
                    "{} of {}",
                    viewing.index + 1,
                    viewing.items.len()
                )))
            });

        deferred(
            anchored().position(point(px(0.), px(0.))).child(
                div()
                    .id("media-viewer")
                    .track_focus(&self.focus)
                    .w(viewport.width)
                    .h(viewport.height)
                    .relative()
                    .bg(hsla(0., 0., 0., 0.86))
                    .occlude()
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| this.key(event, window, cx)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, _| {
                            this.dragging = None;
                            this.dragged = false;
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        let Some(from) = this.dragging else { return };
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.dragging = None;
                            return;
                        }
                        this.offset += event.position - from;
                        this.dragging = Some(event.position);
                        this.dragged = true;
                        this.clamp_offset(window.viewport_size());
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            let on_picture = this.dragging.take().is_some();
                            if !on_picture && !this.dragged {
                                this.close(cx);
                            }
                            this.dragged = false;
                        }),
                    )
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                        let viewport = window.viewport_size();
                        let delta = event.delta.pixel_delta(px(20.));
                        if event.modifiers.platform {
                            let scale = this.scale.unwrap_or(this.fit(viewport));
                            let factor = 1. + f32::from(delta.y) / 200.;
                            this.zoom_to(scale * factor, Some(event.position), viewport, cx);
                            return;
                        }
                        this.offset += delta;
                        this.clamp_offset(viewport);
                        cx.notify();
                    }))
                    .on_pinch(cx.listener(|this, event: &PinchEvent, window, cx| {
                        let viewport = window.viewport_size();
                        let scale = this.scale.unwrap_or(this.fit(viewport));
                        this.zoom_to(scale * (1. + event.delta), Some(event.position), viewport, cx);
                    }))
                    .child(content)
                    .when(many, |viewer| {
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
                                .child(
                                    Self::round_button("previous", "chevron.left", "Previous (←)")
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .on_click(cx.listener(|this, _, _, cx| this.step(-1, cx))),
                                )
                                .child(
                                    Self::round_button("next", "chevron.right", "Next (→)")
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                        .on_click(cx.listener(|this, _, _, cx| this.step(1, cx))),
                                ),
                        )
                    })
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
                    .child(
                        div().absolute().top(px(crate::theme::TOP_BAR + 10.)).right(px(10.)).child(
                            Self::round_button("close-viewer", "xmark", "Close (Esc)")
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                        ),
                    ),
            ),
        )
        .with_priority(2)
        .into_any_element()
    }
}
