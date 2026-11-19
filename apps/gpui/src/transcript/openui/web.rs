//! HtmlView and YouTube. A native web view lies over the window, where the transcript can't clip
//! it, so the row shows a still (the view's title, or the video's thumbnail) and the live page
//! opens over the row while the user works with it, until Esc, a click outside, or the row leaving
//! the transcript closes it. Under a program's page, "Full screen" moves that web view over the
//! whole window and back. Where there is no web view (Linux), the page opens in the browser.

use std::collections::HashMap;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::tree::TextStyle;
use motile_core::render::openui::web::{self, PageStyle};

use super::UiCx;
use super::style::{self, SMALL};
use crate::media::pictures::{Picture, Pictures, Shown};
use crate::theme::{self, ControlSize, IconSize, colors};
use crate::ui::{ActionButton, Spinner};

pub use live::full_screen;

const PLAY_SYMBOL: f32 = IconSize::Hero.side();
/// The pill under a program's page that opens it over the window, and the room above it.
const PILL: ControlSize = ControlSize::Small;
const PILL_GAP: f32 = 6.;

/// The heights pages reported, by row and node, so that a still keeps the height of its page.
#[derive(Default)]
struct Heights(HashMap<String, f32>);

impl Global for Heights {}

fn reported(key: &str, app: &App) -> Option<f32> {
    app.try_global::<Heights>().and_then(|heights| heights.0.get(key).copied())
}

/// The program's page: its `height`, else what the page reported, 360 until it does, leaving room
/// above and below it in the window to scroll the transcript past it. Under it, centred, the pill
/// that opens it over the window.
#[allow(clippy::too_many_arguments)]
pub fn html_view(
    element: Stateful<Div>,
    id: &str,
    html: &str,
    height: Option<f32>,
    title: &str,
    streaming: bool,
    radius: f32,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let c = colors(app);
    let key = format!("{}/{id}", cx.row);
    let height =
        height.or_else(|| reported(&key, app)).unwrap_or(web::START_HEIGHT).min(web::max_height(cx.window_height));
    let style = PageStyle {
        dark: c.dark,
        text_size: theme::PROSE_SIZE.points(),
        line_height: theme::PROSE_LINE_HEIGHT,
        color: css(c.foreground),
        background: css(cx.surface.color(c)),
    };
    let html = html.to_string();
    let page = std::rc::Rc::new(move || live::Page::Html(web::html_page(&html, &style, live::BRIDGE)));
    let live = live::page(cx.id(id, "live"), &key, window, app);
    let (background, transcript) = (cx.surface.color(c), cx.transcript.clone());
    let pill = ActionButton::new(cx.id(id, "full-screen"), "Full screen")
        .symbol("maximize-2")
        .symbol_size(PILL.small_symbol())
        .size(PILL)
        .outline()
        .round(true)
        .surface(cx.surface)
        .disabled(streaming)
        .on_click({
            let (live, page, transcript) = (live.clone(), page.clone(), transcript.clone());
            move |_, window, app| live.full_screen(page(), background, transcript.clone(), window, app)
        });
    let card = div()
        .id(cx.id(id, "page"))
        .relative()
        .w_full()
        .h(px(height))
        .rounded(px(radius))
        .border_1()
        .border_color(cx.surface.border(c))
        .bg(cx.surface.color(c))
        .overflow_hidden()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .px(px(16.))
        .text_center()
        .child(live.track());
    let card = if streaming {
        card.flex_row()
            .child(div().text_color(c.muted_foreground).child(Spinner::regular().render(app)))
            .child(SMALL.text_box().text_color(c.muted_foreground).child("Preparing the interactive view"))
    } else if live.expanded(app) {
        card
    } else if let Some(view) = live.view(app) {
        card.child(view)
    } else {
        let button = ActionButton::new(cx.id(id, "open"), live::OPEN).symbol("play.fill");
        let live = live.clone();
        catches_glide(card, cx)
            .child(style::type_of(TextStyle::CardTitle).text_box().text_color(c.foreground).child(title.to_string()))
            .child(SMALL.text_box().text_color(c.muted_foreground).child("Interactive view"))
            .child(button.on_click(move |_, window, app| {
                live.open(page(), background, transcript.clone(), window, app);
            }))
    };
    element
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(PILL_GAP))
        .child(card)
        .child(div().relative().child(live.track_pill()).child(pill))
}

/// YouTube's player, 16:9 and the column's width: its thumbnail, with the player over it once
/// played.
#[allow(clippy::too_many_arguments)]
pub fn youtube(
    element: Stateful<Div>,
    id: &str,
    url: &str,
    title: &str,
    radius: f32,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let c = colors(app);
    let key = format!("{}/{id}", cx.row);
    let frame =
        element.relative().w_full().h(px(cx.width * 9. / 16.)).rounded(px(radius)).overflow_hidden().bg(c.overlay);
    let live = live::page(cx.id(id, "live"), &key, window, app);
    let frame = frame.child(live.track());
    if let Some(view) = live.view(app) {
        return frame.child(view);
    }
    let thumbnail = thumbnail(url, cx.width * window.scale_factor(), app)
        .map(|shown| img(ImageSource::Render(shown.image)).size_full().object_fit(ObjectFit::Cover));
    let page = live::Page::YouTube { url: url.to_string(), title: title.to_string() };
    let symbol = canvas(
        |_, _, _| {},
        move |bounds, _, window, app| {
            let side = px(PLAY_SYMBOL);
            let center = bounds.center();
            let bounds = Bounds::new(point(center.x - side / 2., center.y - side / 2.), size(side, side));
            let color = colors(app).foreground_over_picture;
            let path = crate::ui::icons::path("play.circle.fill");
            let _ = window.paint_svg(bounds, path, None, TransformationMatrix::unit(), color, app);
        },
    )
    .absolute()
    .inset_0();
    catches_glide(frame, cx).cursor_pointer().children(thumbnail).child(symbol).on_click({
        let transcript = cx.transcript.clone();
        move |_, window, app| live.open(page.clone(), gpui_kit::black(), transcript.clone(), window, app)
    })
}

/// A press on a page while the user's scroll still moves the transcript is the transcript's: the
/// page doesn't open.
fn catches_glide(element: Stateful<Div>, cx: &UiCx) -> Stateful<Div> {
    let transcript = cx.transcript.clone();
    element.capture_any_mouse_down(move |_, _, app| {
        if transcript.as_ref().and_then(WeakEntity::upgrade).is_some_and(|transcript| transcript.read(app).glides()) {
            app.stop_propagation();
        }
    })
}

/// The video's thumbnail, fetched once into the temporary folder.
fn thumbnail(url: &str, pixels: f32, app: &mut App) -> Option<Shown> {
    let thumbnail = web::thumbnail_url(url)?;
    let video = thumbnail.rsplit('/').nth(1)?;
    let file = std::env::temp_dir().join("motile-youtube").join(format!("{video}.jpg"));
    let max_pixels = pixels.ceil() as u32;
    match Pictures::load_web(&format!("youtube:{video}:{max_pixels}"), &thumbnail, &file, max_pixels, app) {
        Picture::Ready(shown) => Some(shown),
        _ => None,
    }
}

/// A colour as CSS takes it, `#rrggbbaa`.
fn css(color: Hsla) -> String {
    let Rgba { r, g, b, a } = color.to_rgb();
    let byte = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b), byte(a))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod live {
    //! The page in a web view of its own over the row, or over the whole window: one channel to
    //! it, for its height, Esc and its fit; no navigation, other windows, storage or developer
    //! tools.

    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use futures::StreamExt;
    use futures::channel::mpsc::{UnboundedSender, unbounded};
    use gpui_kit::*;
    use gpui_wry::WebView;
    use motile_core::render::openui::web::{self, Bridge};

    use super::Heights;
    use crate::theme::{self, ControlSize};
    use crate::transcript::view::TranscriptView;
    use crate::ui::ActionButton;

    pub const OPEN: &str = "Open";
    pub const BRIDGE: Bridge = Bridge::Wry;
    /// The pill that puts a page open over the window back, at the end of the top bar.
    const EXIT: ControlSize = ControlSize::Large;
    const EXIT_INSET: f32 = 12.;
    /// How long after the window is resized the page is fitted to it again, once it is laid out.
    const REFIT_AFTER: Duration = Duration::from_millis(100);

    #[derive(Clone)]
    pub enum Page {
        Html(String),
        YouTube { url: String, title: String },
    }

    enum Message {
        Page(String),
        Browse(String),
        /// What the fit script answered, asked at that zoom.
        Fit {
            at: f64,
            answer: String,
        },
    }

    /// The row's live page, which lives as long as the row is drawn, or is open over the window.
    pub struct Live {
        view: Option<Entity<WebView>>,
        /// The row's bounds in the window, as last drawn.
        bounds: Option<Bounds<Pixels>>,
        /// Where the rows show, out from under the composer, when the page opened.
        area: Option<Bounds<Pixels>>,
        /// The full screen pill's bounds, a press on which leaves the page open.
        pill: Option<Bounds<Pixels>>,
        transcript: Option<WeakEntity<TranscriptView>>,
        /// Open over the window.
        expanded: bool,
        zoom: f64,
        /// The size of the page over the window, as last drawn.
        expanded_size: Option<Size<Pixels>>,
        sender: Option<UnboundedSender<Message>>,
        _messages: Option<Task<()>>,
        _escape: Option<Subscription>,
        _refit: Option<Task<()>>,
    }

    impl Default for Live {
        fn default() -> Self {
            Self {
                view: None,
                bounds: None,
                area: None,
                pill: None,
                transcript: None,
                expanded: false,
                zoom: 1.,
                expanded_size: None,
                sender: None,
                _messages: None,
                _escape: None,
                _refit: None,
            }
        }
    }

    /// The page open over a window, which that window draws.
    #[derive(Default)]
    struct Expanded(Option<(AnyWindowHandle, Entity<Live>)>);

    impl Global for Expanded {}

    #[derive(Clone)]
    pub struct Handle {
        state: Entity<Live>,
        key: String,
    }

    pub fn page(id: ElementId, key: &str, window: &mut Window, app: &mut App) -> Handle {
        Handle { state: window.use_keyed_state(id, app, |_, _| Live::default()), key: key.to_string() }
    }

    impl Handle {
        /// The live page over the part of the row that shows, while it is open there.
        pub fn view(&self, app: &App) -> Option<AnyElement> {
            let live = self.state.read(app);
            let view = live.view.clone().filter(|_| !live.expanded)?;
            let (top, bottom) = match (live.bounds, live.area) {
                (Some(row), Some(area)) => {
                    ((area.top() - row.top()).max(px(0.)), (row.bottom() - area.bottom()).max(px(0.)))
                }
                _ => (px(0.), px(0.)),
            };
            let state = self.state.clone();
            let outside = canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let state = state.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, app| {
                        let on_pill = state.read(app).pill.is_some_and(|pill| pill.contains(&event.position));
                        if phase == DispatchPhase::Capture && !bounds.contains(&event.position) && !on_pill {
                            state.update(app, |live, cx| live.close(cx));
                            window.refresh();
                        }
                    });
                },
            )
            .absolute()
            .inset_0();
            Some(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(top)
                    .bottom(bottom)
                    .child(view)
                    .child(outside)
                    .into_any_element(),
            )
        }

        pub fn expanded(&self, app: &App) -> bool {
            self.state.read(app).expanded
        }

        /// Keeps the row's bounds, and closes the page once the row moves, unless it is open over
        /// the window.
        pub fn track(&self) -> AnyElement {
            let state = self.state.clone();
            canvas(
                move |bounds, window, app| {
                    let live = state.read(app);
                    let moved = live.bounds.is_some_and(|last| last.origin != bounds.origin);
                    let resized = live.bounds.is_some_and(|last| last.size != bounds.size);
                    let open = live.view.is_some() && !live.expanded;
                    state.update(app, |live, _| live.bounds = Some(bounds));
                    if open && moved {
                        let state = state.clone();
                        window.on_next_frame(move |_, app| state.update(app, |live, cx| live.close(cx)));
                    } else if open && resized {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
            .into_any_element()
        }

        /// Keeps the full screen pill's bounds.
        pub fn track_pill(&self) -> AnyElement {
            let state = self.state.clone();
            canvas(move |bounds, _, app| state.update(app, |live, _| live.pill = Some(bounds)), |_, _, _, _| {})
                .absolute()
                .inset_0()
                .into_any_element()
        }

        /// Opens the page over the row, scrolled into the transcript's view first.
        pub fn open(
            &self,
            page: Page,
            background: Hsla,
            transcript: Option<WeakEntity<TranscriptView>>,
            window: &mut Window,
            app: &mut App,
        ) {
            let (sender, mut receiver) = unbounded();
            let webview = match build(&page, background, sender.clone(), window) {
                Ok(webview) => webview,
                Err(error) => {
                    tracing::warn!("couldn't open a web view: {error}");
                    return;
                }
            };
            let view = app.new(|cx| WebView::new(webview, window, cx));
            let (state, key) = (self.state.downgrade(), self.key.clone());
            let messages = window.spawn(app, async move |cx| {
                while let Some(message) = receiver.next().await {
                    let updated = match message {
                        Message::Browse(url) => {
                            let _ = cx.update(|_, app| app.open_url(&url));
                            continue;
                        }
                        Message::Page(text) => state.update(cx, |live, cx| live.heard(&text, &key, cx)),
                        Message::Fit { at, answer } => state.update(cx, |live, cx| live.fitted(at, &answer, cx)),
                    };
                    if updated.is_err() {
                        break;
                    }
                }
            });
            let closing = self.state.downgrade();
            let escape = app.intercept_keystrokes(move |event, _, app| {
                if event.keystroke.key != "escape" || event.keystroke.modifiers.modified() {
                    return;
                }
                app.stop_propagation();
                let closing = closing.clone();
                app.defer(move |app| {
                    let _ = closing.update(app, |live, cx| live.escaped(cx));
                });
            });
            self.state.update(app, |live, cx| {
                live.view = Some(view);
                live.transcript = transcript;
                live.sender = Some(sender);
                live.zoom = 1.;
                live._messages = Some(messages);
                live._escape = Some(escape);
                live.place(cx);
                cx.notify();
            });
        }

        /// Opens the page over the window, as it is in the row when it is open there.
        pub fn full_screen(
            &self,
            page: Page,
            background: Hsla,
            transcript: Option<WeakEntity<TranscriptView>>,
            window: &mut Window,
            app: &mut App,
        ) {
            if self.state.read(app).view.is_none() {
                self.open(page, background, transcript, window, app);
            }
            let Some(view) = self.state.read(app).view.clone() else { return };
            let _ = view.read(app).focus();
            self.state.update(app, |live, cx| {
                live.expanded = true;
                live.expanded_size = None;
                cx.notify();
            });
            if let Some(Some((_, other))) = app.try_global::<Expanded>().map(|expanded| expanded.0.clone())
                && other != self.state
            {
                other.update(app, |live, cx| live.collapse(cx));
            }
            app.set_global(Expanded(Some((window.window_handle(), self.state.clone()))));
            window.refresh();
        }
    }

    /// The page open over this window, under its top bar, which has the pill that puts it back at
    /// its end; its middle moves the window, as the Mac's does.
    pub fn full_screen(moving: &Rc<Cell<bool>>, window: &mut Window, app: &mut App) -> Option<AnyElement> {
        let (handle, state) = app.try_global::<Expanded>()?.0.clone()?;
        if handle != window.window_handle() {
            return None;
        }
        let view = state.read(app).view.clone()?;
        let c = theme::colors(app);
        let area = crate::window_frame::content_bounds(window);
        let trailing = EXIT_INSET + crate::window_frame::trailing_room(window, app);
        let closes = state.clone();
        let exit = ActionButton::new("exit-full-screen", "Exit full screen")
            .symbol("minimize-2")
            .symbol_size(EXIT.small_symbol())
            .size(EXIT)
            .outline()
            .round(true)
            .help("Exit full screen (Esc)")
            .on_click(move |_, _, app| closes.update(app, |live, cx| live.collapse(cx)));
        let resized = canvas(
            move |bounds, _, app| {
                if state.read(app).expanded_size == Some(bounds.size) {
                    return;
                }
                let state = state.clone();
                app.defer(move |app| state.update(app, |live, cx| live.resized(bounds.size, cx)));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let cover = div()
            .id("full-screen-page")
            .occlude()
            .relative()
            .w(area.size.width)
            .h(area.size.height)
            .bg(c.background)
            .child(crate::ui::window_drag_strip("full-screen-top-bar", theme::TOP_BAR, moving))
            .child(div().absolute().top(px(theme::TOP_BAR)).left_0().right_0().bottom_0().child(resized).child(view))
            .child(div().absolute().top_0().right(px(trailing)).h(px(theme::TOP_BAR)).flex().items_center().child(exit))
            .children(crate::window_frame::buttons(window, app));
        Some(deferred(anchored().position(area.origin).child(cover)).with_priority(4).into_any_element())
    }

    impl Live {
        fn close(&mut self, cx: &mut Context<Self>) {
            if self.expanded {
                self.expanded = false;
                self.leave_window(cx);
            }
            if self.view.take().is_some() {
                self.sender = None;
                self._messages = None;
                self._escape = None;
                self._refit = None;
                cx.notify();
            }
        }

        /// Esc puts a page open over the window back in its row, and closes one there.
        fn escaped(&mut self, cx: &mut Context<Self>) {
            if self.expanded { self.collapse(cx) } else { self.close(cx) }
        }

        /// What the page said: its height, which the row takes while the page is there, or that
        /// it asks to be closed.
        fn heard(&mut self, text: &str, key: &str, cx: &mut Context<Self>) {
            if web::asks_to_close(text) {
                return self.escaped(cx);
            }
            let Some(height) = web::reported_height(text) else { return };
            if self.expanded {
                return self.refit(cx);
            }
            cx.default_global::<Heights>().0.insert(key.to_string(), height);
            cx.notify();
        }

        /// Puts the page back in its row, at its own size, scrolled into the transcript's view.
        fn collapse(&mut self, cx: &mut Context<Self>) {
            if !self.expanded {
                return;
            }
            self.expanded = false;
            self._refit = None;
            self.zoom = 1.;
            if let Some(view) = &self.view {
                let _ = view.read(cx).zoom(1.);
                let _ = view.read(cx).focus_parent();
            }
            self.leave_window(cx);
            self.place(cx);
            cx.notify();
        }

        /// The window no longer draws this page over it.
        fn leave_window(&self, cx: &mut Context<Self>) {
            let this = cx.entity();
            if cx
                .try_global::<Expanded>()
                .and_then(|expanded| expanded.0.as_ref())
                .is_some_and(|(_, live)| *live == this)
            {
                cx.set_global(Expanded::default());
                cx.refresh_windows();
            }
        }

        /// Scrolls the row into the transcript's view, where the page then lies.
        fn place(&mut self, cx: &mut Context<Self>) {
            let Some(transcript) = self.transcript.as_ref().and_then(WeakEntity::upgrade) else { return };
            if let Some(row) = self.bounds {
                let distance = transcript.update(cx, |transcript, cx| transcript.reveal(row, cx));
                self.bounds = Some(Bounds::new(point(row.origin.x, row.origin.y - distance), row.size));
            }
            self.area = Some(transcript.read(cx).shown_area(cx));
        }

        fn resized(&mut self, size: Size<Pixels>, cx: &mut Context<Self>) {
            if !self.expanded || self.expanded_size == Some(size) {
                return;
            }
            self.expanded_size = Some(size);
            self._refit = Some(cx.spawn(async move |live, cx| {
                cx.background_executor().timer(REFIT_AFTER).await;
                let _ = live.update(cx, |live, cx| live.refit(cx));
            }));
        }

        /// Asks the page how many times larger it can be drawn over the window.
        fn refit(&mut self, cx: &mut Context<Self>) {
            let (Some(view), Some(sender)) = (&self.view, &self.sender) else { return };
            if !self.expanded {
                return;
            }
            let (at, sender) = (self.zoom, sender.clone());
            let _ = view.read(cx).evaluate_script_with_callback(web::FIT, move |answer| {
                let _ = sender.unbounded_send(Message::Fit { at, answer });
            });
        }

        /// Zooms the page by what it answered, between 1 and core's largest, unless the zoom
        /// changed since it was asked; the height it reports then asks again, until the answer
        /// settles.
        fn fitted(&mut self, at: f64, answer: &str, cx: &mut Context<Self>) {
            let Some(view) = &self.view else { return };
            let Ok(ratio) = answer.trim().parse::<f64>() else { return };
            if !self.expanded || at != self.zoom || !ratio.is_finite() {
                return;
            }
            let zoom = (at * ratio).clamp(1., f64::from(web::MAX_FIT));
            if (zoom - self.zoom).abs() <= 0.01 {
                return;
            }
            self.zoom = zoom;
            let _ = view.read(cx).zoom(zoom);
        }
    }

    fn build(
        page: &Page,
        background: Hsla,
        sender: UnboundedSender<Message>,
        window: &Window,
    ) -> anyhow::Result<wry::WebView> {
        let Rgba { r, g, b, .. } = background.to_rgb();
        let byte = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
        let youtube = matches!(page, Page::YouTube { .. });
        let first = Cell::new(true);
        let browse = sender.clone();
        let mut builder = wry::WebViewBuilder::new()
            .with_incognito(true)
            .with_devtools(false)
            .with_autoplay(youtube)
            .with_background_color((byte(r), byte(g), byte(b), 255))
            .with_navigation_handler(move |url| {
                let initial = first.replace(false);
                initial || (youtube && plays_youtube(&url))
            })
            .with_new_window_req_handler(move |url, _| {
                if youtube && is_youtube(&url) {
                    let _ = browse.unbounded_send(Message::Browse(url));
                }
                wry::NewWindowResponse::Deny
            })
            .with_download_started_handler(|_, _| false)
            .with_ipc_handler(move |request| {
                let _ = sender.unbounded_send(Message::Page(request.into_body()));
            });
        let player = match page {
            Page::Html(_) => None,
            Page::YouTube { url, title } => {
                Some(web::page(&web::PageRequest::YouTube { url: url.clone(), title: title.clone(), autoplay: true }))
            }
        };
        builder = match (page, &player) {
            (Page::Html(html), _) => builder.with_html(html.clone()),
            (_, Some(player)) => serve(builder, player),
            (_, None) => builder,
        };
        let webview = builder.build_as_child(window)?;
        #[cfg(target_os = "macos")]
        if let Some(player) = &player {
            load_with_base(&webview, &player.html, player.base.unwrap_or(web::YOUTUBE_BASE));
        }
        Ok(webview)
    }

    /// WebView2 serves the player's page at `https://motile.app/`, through the `motile` protocol.
    #[cfg(target_os = "windows")]
    fn serve<'a>(builder: wry::WebViewBuilder<'a>, player: &web::Page) -> wry::WebViewBuilder<'a> {
        use std::borrow::Cow;
        use wry::WebViewBuilderExtWindows;
        let html = player.html.clone();
        builder
            .with_https_scheme(true)
            .with_custom_protocol("motile".into(), move |_, _| {
                wry::http::Response::builder()
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(Cow::Owned(html.clone().into_bytes()))
                    .unwrap_or_default()
            })
            .with_url("motile://app/")
    }

    /// WebKit loads the player's page at its address once the view is made.
    #[cfg(target_os = "macos")]
    fn serve<'a>(builder: wry::WebViewBuilder<'a>, _: &web::Page) -> wry::WebViewBuilder<'a> {
        builder
    }

    #[cfg(target_os = "macos")]
    fn load_with_base(webview: &wry::WebView, html: &str, base: &str) {
        use objc2::runtime::AnyObject;
        use objc2_foundation::{NSString, NSURL};
        use wry::WebViewExtMacOS;
        let Some(base) = NSURL::URLWithString(&NSString::from_str(base)) else { return };
        let view = webview.webview();
        let html = NSString::from_str(html);
        let _: *mut AnyObject = unsafe { objc2::msg_send![&*view, loadHTMLString: &*html, baseURL: &*base] };
    }

    /// What the player's frame may go to.
    fn plays_youtube(url: &str) -> bool {
        url.starts_with("https://www.youtube-nocookie.com/") || url.starts_with("about:")
    }

    fn is_youtube(url: &str) -> bool {
        ["https://www.youtube.com/", "https://youtube.com/", "https://youtu.be/", "https://m.youtube.com/"]
            .iter()
            .any(|prefix| url.starts_with(prefix))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod live {
    //! No web view here: the page opens in the browser, from a file for a program's page.

    use std::cell::Cell;
    use std::rc::Rc;

    use gpui_kit::*;
    use motile_core::render::openui::web::{self, Bridge};

    use crate::transcript::view::TranscriptView;

    pub const OPEN: &str = "Open in Browser";
    pub const BRIDGE: Bridge = Bridge::Browser;

    #[derive(Clone)]
    pub enum Page {
        Html(String),
        YouTube {
            url: String,
            #[allow(dead_code)]
            title: String,
        },
    }

    #[derive(Clone)]
    pub struct Handle;

    pub fn page(_: ElementId, _: &str, _: &mut Window, _: &mut App) -> Handle {
        Handle
    }

    pub fn full_screen(_: &Rc<Cell<bool>>, _: &mut Window, _: &mut App) -> Option<AnyElement> {
        None
    }

    impl Handle {
        pub fn view(&self, _: &App) -> Option<AnyElement> {
            None
        }

        pub fn expanded(&self, _: &App) -> bool {
            false
        }

        pub fn track(&self) -> AnyElement {
            div().into_any_element()
        }

        pub fn track_pill(&self) -> AnyElement {
            div().into_any_element()
        }

        /// The browser is the page's full screen here.
        pub fn full_screen(
            &self,
            page: Page,
            background: Hsla,
            transcript: Option<WeakEntity<TranscriptView>>,
            window: &mut Window,
            app: &mut App,
        ) {
            self.open(page, background, transcript, window, app);
        }

        pub fn open(&self, page: Page, _: Hsla, _: Option<WeakEntity<TranscriptView>>, _: &mut Window, app: &mut App) {
            match page {
                Page::Html(html) => match written(&html) {
                    Ok(path) => app.open_url(&format!("file://{}", path.display())),
                    Err(error) => tracing::warn!("couldn't write a page to open: {error}"),
                },
                Page::YouTube { url, .. } => {
                    if let Some(watch) = web::watch_url(&url) {
                        app.open_url(&watch);
                    }
                }
            }
        }
    }

    /// The page in a file of the temporary folder that only the user reads. Its name can't be
    /// guessed and the file can't be one that was there, such as another user's link.
    fn written(html: &str) -> std::io::Result<std::path::PathBuf> {
        use std::hash::{BuildHasher, RandomState};
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let name = format!("motile-{:016x}.html", RandomState::new().hash_one(html));
        let path = std::env::temp_dir().join(name);
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
        file.write_all(html.as_bytes())?;
        Ok(path)
    }
}
