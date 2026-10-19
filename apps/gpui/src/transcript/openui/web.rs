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
    if let Some(view) = live.view(app) {
        return card.child(view);
    }
    let still = card
        .child(style::type_of(TextStyle::CardTitle, false).text_box().text_color(c.foreground).child(title.to_string()))
        .child(SMALL.text_box().text_color(c.muted_foreground).child("Interactive view"));
    let button = ActionButton::new(cx.id(id, "open"), live::OPEN).symbol("play.fill");
    let background = cx.surface.color(c);
    still.child(button.on_click(move |_, window, app| {
        live.open(live::Page::Html(open(live::BRIDGE)), background, window, app);
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
    if let Some(view) = live.view(app) {
        return frame.child(view);
    }
    let thumbnail = web::thumbnail_url(url)
        .map(|thumbnail| img(SharedUri::from(thumbnail)).size_full().object_fit(ObjectFit::Cover));
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
    .size_full();
    frame
        .cursor_pointer()
        .children(thumbnail)
        .child(symbol)
        .on_click(move |_, window, app| live.open(page.clone(), gpui_kit::black(), window, app))
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
        _messages: Option<Task<()>>,
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
        /// The live page over the row, and what closes it, while it is open.
        pub fn view(&self, app: &App) -> Option<AnyElement> {
            let view = self.state.read(app).view.clone()?;
            let state = self.state.clone();
            let watch = canvas(
                {
                    let state = state.clone();
                    move |bounds, window, _| {
                        if window.content_mask().bounds.intersect(&bounds).is_empty() {
                            close_soon(&state, window);
                        }
                    }
                },
                move |bounds, _, window, _| {
                    let outside = state.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, app| {
                        if phase == DispatchPhase::Capture && !bounds.contains(&event.position) {
                            outside.update(app, |live, cx| live.close(cx));
                            window.refresh();
                        }
                    });
                    let escape = state.clone();
                    window.on_key_event(move |event: &KeyDownEvent, phase, window, app| {
                        if phase == DispatchPhase::Capture && event.keystroke.key == "escape" {
                            escape.update(app, |live, cx| live.close(cx));
                            app.stop_propagation();
                            window.refresh();
                        }
                    });
                },
            )
            .absolute()
            .size_full();
            Some(div().absolute().inset_0().child(view).child(watch).into_any_element())
        }

        pub fn open(&self, page: Page, background: Hsla, window: &mut Window, app: &mut App) {
            let (sender, mut receiver) = unbounded();
            let webview = match build(&page, background, sender, window) {
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
            self.state.update(app, |live, cx| {
                live.view = Some(view);
                live._messages = Some(messages);
                cx.notify();
            });
        }
    }

    impl Live {
        fn close(&mut self, cx: &mut Context<Self>) {
            if self.view.take().is_some() {
                self._messages = None;
                cx.notify();
            }
        }
    }

    /// Closes the page once this frame is drawn: a frame that is being laid out can't change it.
    fn close_soon(state: &Entity<Live>, window: &mut Window) {
        let state = state.clone();
        window.on_next_frame(move |_, app| state.update(app, |live, cx| live.close(cx)));
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

        pub fn open(&self, page: Page, _: Hsla, _: &mut Window, app: &mut App) {
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
