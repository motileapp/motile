//! HtmlView and YouTube. A native web view lies over the window, where the transcript can't clip
//! it, so the row shows a still (the view's title, or the video's thumbnail) and the live page
//! opens over the row while the user works with it, until Esc, a click outside, or the row leaving
//! the transcript closes it. Where there is no web view (Linux), the page opens in the browser.

use std::collections::HashMap;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::openui::tree::TextStyle;
use motile_core::render::openui::web::{self, Bridge, PageStyle};

use super::UiCx;
use super::style::{self, SMALL};
use crate::media::pictures::{Picture, Pictures, Shown};
use crate::theme::{self, Radius, colors};
use crate::ui::{ActionButton, Spinner};

const PLAY_SYMBOL: f32 = 40.;

/// The heights pages reported, by row and node, so that a still keeps the height of its page.
#[derive(Default)]
struct Heights(HashMap<String, f32>);

impl Global for Heights {}

fn reported(key: &str, app: &App) -> Option<f32> {
    app.try_global::<Heights>().and_then(|heights| heights.0.get(key).copied())
}

/// The program's page: its `height`, else what the page reported, 360 until it does.
#[allow(clippy::too_many_arguments)]
pub fn html_view(
    element: Stateful<Div>,
    id: &str,
    html: &str,
    height: Option<f32>,
    title: &str,
    streaming: bool,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let c = colors(app);
    let key = format!("{}/{id}", cx.row);
    let height = height.or_else(|| reported(&key, app)).unwrap_or(web::START_HEIGHT);
    let card = element
        .relative()
        .w_full()
        .h(px(height))
        .rounded(px(Radius::LG))
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
        .text_center();
    if streaming {
        return card
            .flex_row()
            .child(div().text_color(c.muted_foreground).child(Spinner::regular().render(app)))
            .child(SMALL.text_box().text_color(c.muted_foreground).child("Preparing the interactive view"));
    }
    let style = PageStyle {
        dark: c.dark,
        text_size: theme::PROSE_SIZE,
        line_height: theme::PROSE_LINE_HEIGHT,
        color: css(c.foreground),
        background: css(cx.surface.color(c)),
    };
    let html = html.to_string();
    let open = move |bridge: Bridge| web::html_page(&html, &style, bridge);
    let live = live::page(cx.id(id, "live"), &key, window, app);
    let card = card.child(live.track());
    if let Some(view) = live.view(app) {
        return card.child(view);
    }
    let still = card
        .child(style::type_of(TextStyle::CardTitle, false).text_box().text_color(c.foreground).child(title.to_string()))
        .child(SMALL.text_box().text_color(c.muted_foreground).child("Interactive view"));
    let button = ActionButton::new(cx.id(id, "open"), live::OPEN).symbol("play.fill");
    let (background, transcript) = (cx.surface.color(c), cx.transcript.clone());
    still.child(button.on_click(move |_, window, app| {
        live.open(live::Page::Html(open(live::BRIDGE)), background, transcript.clone(), window, app);
    }))
}

/// YouTube's player, 16:9 and the column's width: its thumbnail, with the player over it once
/// played.
pub fn youtube(
    element: Stateful<Div>,
    id: &str,
    url: &str,
    title: &str,
    cx: &UiCx,
    window: &mut Window,
    app: &mut App,
) -> Stateful<Div> {
    let c = colors(app);
    let key = format!("{}/{id}", cx.row);
    let frame =
        element.relative().w_full().h(px(cx.width * 9. / 16.)).rounded(px(Radius::LG)).overflow_hidden().bg(c.overlay);
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
            let side = px(theme::symbol_side(PLAY_SYMBOL));
            let center = bounds.center();
            let bounds = Bounds::new(point(center.x - side / 2., center.y - side / 2.), size(side, side));
            let color = colors(app).foreground_over_picture;
            let path = crate::ui::icons::path("play.circle.fill");
            let _ = window.paint_svg(bounds, path, None, TransformationMatrix::unit(), color, app);
        },
    )
    .absolute()
    .inset_0();
    frame.cursor_pointer().children(thumbnail).child(symbol).on_click({
        let transcript = cx.transcript.clone();
        move |_, window, app| live.open(page.clone(), gpui_kit::black(), transcript.clone(), window, app)
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
    //! The page in a web view of its own over the row: one channel to it, for its height and
    //! Esc; no navigation, other windows, storage or developer tools.

    use std::cell::Cell;

    use futures::StreamExt;
    use futures::channel::mpsc::{UnboundedSender, unbounded};
    use gpui_kit::*;
    use gpui_wry::WebView;
    use motile_core::render::openui::web::{self, Bridge};

    use super::Heights;
    use crate::transcript::view::TranscriptView;

    pub const OPEN: &str = "Open";
    pub const BRIDGE: Bridge = Bridge::Wry;

    #[derive(Clone)]
    pub enum Page {
        Html(String),
        YouTube { url: String, title: String },
    }

    enum Message {
        Page(String),
        Browse(String),
    }

    /// The row's live page, which lives as long as the row is drawn.
    #[derive(Default)]
    pub struct Live {
        view: Option<Entity<WebView>>,
        /// The row's bounds in the window, as last drawn.
        bounds: Option<Bounds<Pixels>>,
        /// Where the rows show, out from under the composer, when the page opened.
        area: Option<Bounds<Pixels>>,
        _messages: Option<Task<()>>,
        _escape: Option<Subscription>,
    }

    #[derive(Clone)]
    pub struct Handle {
        state: Entity<Live>,
        key: String,
    }

    pub fn page(id: ElementId, key: &str, window: &mut Window, app: &mut App) -> Handle {
        Handle { state: window.use_keyed_state(id, app, |_, _| Live::default()), key: key.to_string() }
    }

    impl Handle {
        /// The live page over the part of the row that shows, while it is open.
        pub fn view(&self, app: &App) -> Option<AnyElement> {
            let live = self.state.read(app);
            let view = live.view.clone()?;
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
                        if phase == DispatchPhase::Capture && !bounds.contains(&event.position) {
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

        /// Keeps the row's bounds, and closes the page once the row moves.
        pub fn track(&self) -> AnyElement {
            let state = self.state.clone();
            canvas(
                move |bounds, window, app| {
                    let moved = state.read(app).bounds.is_some_and(|last| last.origin != bounds.origin);
                    let open = state.read(app).view.is_some();
                    let resized = state.read(app).bounds.is_some_and(|last| last.size != bounds.size);
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
            let webview = match build(&page, background, sender, window) {
                Ok(webview) => webview,
                Err(error) => {
                    tracing::warn!("couldn't open a web view: {error}");
                    return;
                }
            };
            let view = app.new(|cx| WebView::new(webview, window, cx));
            let mut bounds = self.state.read(app).bounds;
            let mut area = None;
            if let Some(transcript) = transcript.and_then(|transcript| transcript.upgrade()) {
                if let Some(row) = bounds {
                    let distance = transcript.update(app, |transcript, cx| transcript.reveal(row, cx));
                    bounds = Some(Bounds::new(point(row.origin.x, row.origin.y - distance), row.size));
                }
                area = Some(transcript.read(app).shown_area(app));
            }
            let (state, key) = (self.state.downgrade(), self.key.clone());
            let messages = window.spawn(app, async move |cx| {
                while let Some(message) = receiver.next().await {
                    let text = match message {
                        Message::Browse(url) => {
                            let _ = cx.update(|_, app| app.open_url(&url));
                            continue;
                        }
                        Message::Page(text) => text,
                    };
                    let (closes, height, key) = (web::asks_to_close(&text), web::reported_height(&text), key.clone());
                    let updated = state.update(cx, |live, cx| {
                        if let Some(height) = height {
                            cx.default_global::<Heights>().0.insert(key, height);
                            cx.notify();
                        }
                        if closes {
                            live.close(cx);
                        }
                    });
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
                    let _ = closing.update(app, |live, cx| live.close(cx));
                });
            });
            self.state.update(app, |live, cx| {
                live.view = Some(view);
                live.bounds = bounds;
                live.area = area;
                live._messages = Some(messages);
                live._escape = Some(escape);
                cx.notify();
            });
        }
    }

    impl Live {
        fn close(&mut self, cx: &mut Context<Self>) {
            if self.view.take().is_some() {
                self._messages = None;
                self._escape = None;
                cx.notify();
            }
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

    use gpui_kit::*;
    use motile_core::render::openui::web::{self, Bridge};

    use crate::transcript::view::TranscriptView;

    pub const OPEN: &str = "Open in Browser";
    pub const BRIDGE: Bridge = Bridge::Browser;

    #[derive(Clone)]
    pub enum Page {
        Html(String),
        YouTube { url: String, title: String },
    }

    #[derive(Clone)]
    pub struct Handle;

    pub fn page(_: ElementId, _: &str, _: &mut Window, _: &mut App) -> Handle {
        Handle
    }

    impl Handle {
        pub fn view(&self, _: &App) -> Option<AnyElement> {
            None
        }

        pub fn track(&self) -> AnyElement {
            div().into_any_element()
        }

        pub fn open(&self, page: Page, _: Hsla, _: Option<WeakEntity<TranscriptView>>, _: &mut Window, app: &mut App) {
            match page {
                Page::Html(html) => {
                    let name = format!("motile-{:016x}.html", hash(&html));
                    let path = std::env::temp_dir().join(name);
                    match std::fs::write(&path, html) {
                        Ok(()) => app.open_url(&format!("file://{}", path.display())),
                        Err(error) => tracing::warn!("couldn't write a page to open: {error}"),
                    }
                }
                Page::YouTube { url, .. } => {
                    if let Some(watch) = web::watch_url(&url) {
                        app.open_url(&watch);
                    }
                }
            }
        }
    }

    fn hash(text: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }
}
