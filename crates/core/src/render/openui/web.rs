//! The pages HtmlView and YouTube show, each client in a sandboxed web view of its own: the
//! program's HTML under open-intelligent-ui's policy, and YouTube's player in a small page whose
//! https address names the app, since the player refuses to play for a page without one.

use serde::{Deserialize, Serialize};

/// What a program's page may load: its own inline code, a few script CDNs, fonts, pictures, and
/// nothing else from the network. It comes before the program's markup, so it covers all of it.
pub const CSP: &str = "default-src 'none'; \
    script-src 'unsafe-inline' https://cdn.jsdelivr.net https://unpkg.com https://cdnjs.cloudflare.com https://esm.sh; \
    style-src 'unsafe-inline' https://cdn.jsdelivr.net https://unpkg.com https://cdnjs.cloudflare.com https://esm.sh https://fonts.googleapis.com; \
    font-src https://fonts.gstatic.com data:; \
    img-src data: blob: https:; \
    media-src data: blob:; \
    connect-src https://cdn.jsdelivr.net https://unpkg.com https://cdnjs.cloudflare.com https://esm.sh; \
    frame-src 'none'; \
    form-action 'none'; \
    base-uri 'none'";

/// The player's page lets in nothing but the player.
const YOUTUBE_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; frame-src https://www.youtube-nocookie.com";

/// The address the player's page is loaded at: YouTube's player wants to know the page it plays in.
pub const YOUTUBE_BASE: &str = "https://motile.app/";

/// The name of the one channel a page has to the client: its height, and whose a touch or a wheel is.
pub const HANDLER: &str = "motile";
pub const HEIGHT_MESSAGE: &str = "iui-html-view-height";
/// The page takes the finger on it (`hold`), as `touch-action` or `preventDefault()` say, so the
/// transcript doesn't scroll under it, until the last finger lifts (`hold: false`).
pub const HOLD_MESSAGE: &str = "iui-html-view-hold";
/// Whether the page took a wheel event (`taken`): prevented it, or scrolled with it. The Mac app
/// hands the rest of the gesture to the transcript when it didn't.
pub const WHEEL_MESSAGE: &str = "iui-html-view-wheel";
/// Esc in the page, for the GPUI app, whose overlay can't hear the keys the web view takes.
pub const CLOSE_MESSAGE: &str = "iui-html-view-close";

/// How tall a page's view is: what the page reports, between these.
pub const MIN_HEIGHT: f32 = 80.;
pub const MAX_HEIGHT: f32 = 2400.;
/// Until the page has reported.
pub const START_HEIGHT: f32 = 360.;
/// What a page leaves of the window above and below it, so that the transcript can always be
/// scrolled past it: a share of the window's height, at least the least.
pub const WINDOW_MARGIN: f32 = 0.2;
pub const LEAST_MARGIN: f32 = 120.;

/// The tallest a page's view is in a window `window` tall; a taller page scrolls inside it.
pub fn max_height(window: f32) -> f32 {
    let margin = (window * WINDOW_MARGIN).max(LEAST_MARGIN);
    (window - 2. * margin).clamp(MIN_HEIGHT, MAX_HEIGHT)
}

/// How a page reaches its client.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Bridge {
    /// `WKScriptMessageHandler` in a content world of its own: the reporter runs in that world,
    /// out of the page's reach, so it isn't in the page.
    WebKit,
    /// `WebViewCompat.addWebMessageListener`, an object the page sees.
    Android,
    /// Wry's IPC handler, `window.ipc`.
    Wry,
    /// None: the page opens in the browser, where nothing hears it.
    Browser,
}

/// The text around the view, which the page's own text takes: sizes in CSS pixels, colours as
/// `#rrggbb` or `#rrggbbaa`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct PageStyle {
    pub dark: bool,
    pub text_size: f32,
    pub line_height: f32,
    pub color: String,
    pub background: String,
}

/// What a client asks `page_json` for.
#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageRequest {
    Html {
        html: String,
        bridge: Bridge,
        #[serde(flatten)]
        style: PageStyle,
    },
    #[serde(rename = "youtube")]
    YouTube {
        url: String,
        title: String,
        /// It plays as it opens: the user asked for it.
        #[serde(default)]
        autoplay: bool,
    },
}

/// A page and how to load it.
#[derive(Serialize, PartialEq, Debug)]
pub struct Page {
    pub html: String,
    /// The address to load it at; none for a program's page, which the client serves from an
    /// opaque origin of its own.
    pub base: Option<&'static str>,
    /// The reporter to run in the bridge's own world, for `Bridge::WebKit`.
    pub script: Option<String>,
    pub handler: &'static str,
    pub height: Heights,
}

#[derive(Serialize, PartialEq, Debug)]
pub struct Heights {
    pub start: f32,
    pub min: f32,
    pub max: f32,
}

const HEIGHTS: Heights = Heights { start: START_HEIGHT, min: MIN_HEIGHT, max: MAX_HEIGHT };

/// A program's page: the policy, the view's text style, the height reporter where the page
/// carries it, then the program's markup.
pub fn html_page(html: &str, style: &PageStyle, bridge: Bridge) -> String {
    let scheme = if style.dark { "dark" } else { "light" };
    let font = format!(
        "{}px/{}px -apple-system,BlinkMacSystemFont,\"Segoe UI\",Roboto,Helvetica,Arial,sans-serif",
        style.text_size, style.line_height
    );
    let background = colour(&style.background).map(|colour| format!("html{{background:{colour}}}")).unwrap_or_default();
    let color = colour(&style.color).map(|colour| format!(";color:{colour}")).unwrap_or_default();
    let reporter = match bridge {
        Bridge::WebKit | Bridge::Browser => String::new(),
        bridge => format!("<script>{}</script>", reporter(bridge)),
    };
    format!(
        "<!doctype html><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <style>:root{{color-scheme:{scheme}}}html,body{{margin:0}}html{{overscroll-behavior:none}}{background}body{{font:{font}{color}}}</style>\
         {reporter}{html}"
    )
}

/// The script that reports the page's height whenever it changes, by `bridge`, and where there are
/// fingers or a wheel, whose they are.
pub fn reporter(bridge: Bridge) -> String {
    let post = match bridge {
        Bridge::WebKit => format!("window.webkit.messageHandlers.{HANDLER}.postMessage(JSON.stringify(m))"),
        Bridge::Android => format!("{HANDLER}.postMessage(JSON.stringify(m))"),
        Bridge::Wry => "window.ipc.postMessage(JSON.stringify(m))".to_owned(),
        Bridge::Browser => return String::new(),
    };
    let close = match bridge {
        Bridge::Wry => format!(
            "addEventListener(\"keydown\",function(e){{if(e.key===\"Escape\")post({{type:\"{CLOSE_MESSAGE}\"}})}},true);"
        ),
        _ => String::new(),
    };
    let touch = match bridge {
        Bridge::WebKit | Bridge::Android => TOUCH.replace("HOLD", HOLD_MESSAGE),
        _ => String::new(),
    };
    let wheel = match bridge {
        Bridge::WebKit => WHEEL.replace("WHEEL", WHEEL_MESSAGE),
        _ => String::new(),
    };
    format!(
        "(function(){{var last=0,quiet={{capture:true,passive:true}};function post(m){{try{{{post}}}catch(e){{}}}}\
         function send(){{var h=Math.ceil(document.documentElement.getBoundingClientRect().height);\
         if(Math.abs(h-last)>1){{last=h;post({{type:\"{HEIGHT_MESSAGE}\",height:h}})}}}}\
         {close}{touch}{wheel}addEventListener(\"load\",send);new ResizeObserver(send).observe(document.documentElement);send()}})()"
    )
}

/// A finger on an element whose `touch-action` keeps the page from panning up and down, or one
/// the page prevented, is the page's.
const TOUCH: &str = "var held=false;function hold(h){if(h!==held){held=h;post({type:\"HOLD\",hold:h})}}\
    function pinned(t){for(var e=t;e&&e.nodeType===1;e=e.parentElement){var a=getComputedStyle(e).touchAction;\
    if(a!==\"auto\"&&a!==\"manipulation\"&&!/pan-y|pan-up|pan-down/.test(a))return true}return false}\
    function prevented(e){setTimeout(function(){if(e.defaultPrevented)hold(true)},0)}\
    function lifted(e){if(!e.touches.length)hold(false)}\
    addEventListener(\"touchstart\",function(e){if(pinned(e.target))hold(true);prevented(e)},quiet);\
    addEventListener(\"touchmove\",prevented,quiet);addEventListener(\"touchend\",lifted,quiet);\
    addEventListener(\"touchcancel\",lifted,quiet);";

/// A wheel the page prevented, or that scrolls something in it that way, is the page's.
const WHEEL: &str = "function scrolls(t,x,y){var up=Math.abs(y)>=Math.abs(x),d=up?y:x,root=document.scrollingElement;\
    for(var e=t;e&&e.nodeType===1;e=e.parentElement){var s=getComputedStyle(e);\
    if(e!==root&&!/auto|scroll|overlay/.test(up?s.overflowY:s.overflowX))continue;\
    var at=up?e.scrollTop:e.scrollLeft,room=up?e.scrollHeight-e.clientHeight:e.scrollWidth-e.clientWidth;\
    if(room>1&&(d>0?at<room-1:d<0&&at>0))return true}return false}\
    addEventListener(\"wheel\",function(e){var s=scrolls(e.target,e.deltaX,e.deltaY);\
    setTimeout(function(){post({type:\"WHEEL\",taken:s||e.defaultPrevented})},0)},quiet);";

/// The player of an embed address core resolved, filling its page.
pub fn youtube_page(url: &str, title: &str, autoplay: bool) -> String {
    let url = if autoplay { format!("{url}?autoplay=1") } else { url.to_owned() };
    format!(
        "<!doctype html><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{YOUTUBE_CSP}\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <style>html,body{{margin:0;overflow:hidden;background:#000}}iframe{{position:fixed;inset:0;width:100%;height:100%;border:0}}</style>\
         <iframe src=\"{}\" title=\"{}\" allow=\"accelerometer; encrypted-media; gyroscope; picture-in-picture; fullscreen\" \
         allowfullscreen referrerpolicy=\"strict-origin-when-cross-origin\"></iframe>",
        escape(&url),
        escape(title)
    )
}

/// The video's page on YouTube, for where no web view can play it.
pub fn watch_url(embed: &str) -> Option<String> {
    let id = embed.strip_prefix("https://www.youtube-nocookie.com/embed/")?;
    Some(format!("https://www.youtube.com/watch?v={id}"))
}

/// Its thumbnail, as YouTube keeps it.
pub fn thumbnail_url(embed: &str) -> Option<String> {
    let id = embed.strip_prefix("https://www.youtube-nocookie.com/embed/")?;
    Some(format!("https://i.ytimg.com/vi/{id}/hqdefault.jpg"))
}

/// A height the page reported, as the view takes it; none for any other message.
pub fn reported_height(message: &str) -> Option<f32> {
    let message: serde_json::Value = serde_json::from_str(message).ok()?;
    if message.get("type")?.as_str()? != HEIGHT_MESSAGE {
        return None;
    }
    let height = message.get("height")?.as_f64().filter(|height| height.is_finite())?;
    Some((height as f32).clamp(MIN_HEIGHT, MAX_HEIGHT))
}

/// Whether the page asked to be closed.
pub fn asks_to_close(message: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(message)
        .ok()
        .and_then(|message| message.get("type")?.as_str().map(|kind| kind == CLOSE_MESSAGE))
        .unwrap_or(false)
}

pub fn page(request: &PageRequest) -> Page {
    match request {
        PageRequest::Html { html, bridge, style } => Page {
            html: html_page(html, style, *bridge),
            base: None,
            script: (*bridge == Bridge::WebKit).then(|| reporter(Bridge::WebKit)),
            handler: HANDLER,
            height: HEIGHTS,
        },
        PageRequest::YouTube { url, title, autoplay } => Page {
            html: youtube_page(url, title, *autoplay),
            base: Some(YOUTUBE_BASE),
            script: None,
            handler: HANDLER,
            height: HEIGHTS,
        },
    }
}

/// [`page`] from a [`PageRequest`] as JSON to its [`Page`] as JSON; an empty object when it can't
/// be read.
pub fn page_json(input: &str) -> String {
    match serde_json::from_str::<PageRequest>(input) {
        Ok(request) => serde_json::to_string(&page(&request)).unwrap_or_default(),
        Err(error) => {
            tracing::warn!("couldn't read a page to build: {error}");
            "{}".to_owned()
        }
    }
}

/// A CSS colour the client gave, if it is one.
fn colour(text: &str) -> Option<&str> {
    let digits = text.strip_prefix('#')?;
    let valid = matches!(digits.len(), 3 | 4 | 6 | 8) && digits.chars().all(|c| c.is_ascii_hexdigit());
    valid.then_some(text)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> PageStyle {
        PageStyle {
            dark: true,
            text_size: 14.,
            line_height: 21.,
            color: "#ececec".into(),
            background: "#171717".into(),
        }
    }

    #[test]
    fn the_policy_comes_before_the_programs_markup() {
        let page = html_page("<script>alert(1)</script>", &style(), Bridge::Android);
        let policy = page.find("Content-Security-Policy").unwrap();
        let markup = page.find("<script>alert(1)").unwrap();
        assert!(page.starts_with("<!doctype html><meta charset=\"utf-8\">"));
        assert!(policy < markup);
        assert!(page.contains(&format!("content=\"{CSP}\"")));
        assert!(page.contains(":root{color-scheme:dark}"));
        assert!(page.contains("html{background:#171717}"));
        assert!(page.contains("body{font:14px/21px -apple-system"));
        assert!(page.contains(";color:#ececec}"));
        assert!(page.ends_with("<script>alert(1)</script>"));
    }

    #[test]
    fn the_policy_is_open_intelligent_uis() {
        let rules: Vec<&str> = CSP.split("; ").map(str::trim).collect();
        assert_eq!(rules[0], "default-src 'none'");
        assert!(rules.contains(&"frame-src 'none'"));
        assert!(rules.contains(&"form-action 'none'"));
        assert!(rules.contains(&"base-uri 'none'"));
        assert!(rules.contains(&"img-src data: blob: https:"));
        assert!(rules.contains(&"media-src data: blob:"));
        assert!(rules.contains(&"font-src https://fonts.gstatic.com data:"));
        assert_eq!(rules.len(), 10);
    }

    #[test]
    fn each_bridge_reports_through_its_own_channel() {
        let android = html_page("", &style(), Bridge::Android);
        assert!(android.contains("motile.postMessage(JSON.stringify(m))"));
        assert!(!android.contains(CLOSE_MESSAGE));
        assert!(android.contains(HOLD_MESSAGE));
        assert!(!android.contains(WHEEL_MESSAGE));
        let wry = html_page("", &style(), Bridge::Wry);
        assert!(wry.contains("window.ipc.postMessage"));
        assert!(wry.contains(CLOSE_MESSAGE));
        assert!(!wry.contains(HOLD_MESSAGE));
        let webkit = reporter(Bridge::WebKit);
        assert!(webkit.contains(HOLD_MESSAGE) && webkit.contains(WHEEL_MESSAGE));
        let webkit = html_page("", &style(), Bridge::WebKit);
        assert!(!webkit.contains("<script>"), "WebKit's reporter runs in its own world");
        assert!(!html_page("", &style(), Bridge::Browser).contains("<script>"));
        let page = page(&PageRequest::Html { html: String::new(), bridge: Bridge::WebKit, style: style() });
        assert!(page.script.unwrap().contains("window.webkit.messageHandlers.motile.postMessage"));
        assert_eq!(page.base, None);
    }

    #[test]
    fn a_page_leaves_room_to_scroll_past_it() {
        assert_eq!(max_height(956.), 956. - 2. * 956. * 0.2);
        assert_eq!(max_height(800.), 480.);
        assert_eq!(max_height(440.), 200.);
        assert_eq!(max_height(200.), MIN_HEIGHT);
        assert_eq!(max_height(10_000.), MAX_HEIGHT);
    }

    #[test]
    fn a_colour_that_isnt_one_is_left_out() {
        let style = PageStyle { color: "red;}</style><script>".into(), background: "#12345".into(), ..style() };
        let page = html_page("", &style, Bridge::Wry);
        assert!(!page.contains("red;"));
        assert!(!page.contains("html{background"));
    }

    #[test]
    fn the_player_plays_in_a_page_with_an_address() {
        let page = page(&PageRequest::YouTube {
            url: "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ".into(),
            title: "A \"music\" <video>".into(),
            autoplay: false,
        });
        assert_eq!(page.base, Some("https://motile.app/"));
        assert!(page.html.contains("<iframe src=\"https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ\""));
        assert!(page.html.contains("title=\"A &quot;music&quot; &lt;video&gt;\""));
        assert!(page.html.contains("frame-src https://www.youtube-nocookie.com"));
        let playing = youtube_page("https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ", "", true);
        assert!(playing.contains("embed/dQw4w9WgXcQ?autoplay=1\""));
    }

    #[test]
    fn a_video_has_a_page_and_a_thumbnail_on_youtube() {
        let embed = "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ";
        assert_eq!(watch_url(embed).as_deref(), Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert_eq!(thumbnail_url(embed).as_deref(), Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg"));
        assert_eq!(watch_url("https://example.com/x"), None);
    }

    #[test]
    fn a_reported_height_is_kept_within_bounds() {
        assert_eq!(reported_height(r#"{"type":"iui-html-view-height","height":512}"#), Some(512.));
        assert_eq!(reported_height(r#"{"type":"iui-html-view-height","height":12}"#), Some(80.));
        assert_eq!(reported_height(r#"{"type":"iui-html-view-height","height":99999}"#), Some(2400.));
        assert_eq!(reported_height(r#"{"type":"other","height":512}"#), None);
        assert_eq!(reported_height("512"), None);
        assert!(asks_to_close(r#"{"type":"iui-html-view-close"}"#));
        assert!(!asks_to_close(r#"{"type":"iui-html-view-height","height":1}"#));
    }

    #[test]
    fn a_request_as_json_answers_a_page_as_json() {
        let answer = page_json(
            r##"{"kind":"html","html":"<p>hi</p>","bridge":"android","dark":false,"text_size":16,"line_height":24,"color":"#000000","background":"#ffffff"}"##,
        );
        let answer: serde_json::Value = serde_json::from_str(&answer).unwrap();
        assert!(answer["html"].as_str().unwrap().ends_with("<p>hi</p>"));
        assert!(answer["html"].as_str().unwrap().contains("color-scheme:light"));
        assert_eq!(answer["handler"], "motile");
        assert_eq!(answer["height"]["start"], 360.);
        assert_eq!(page_json("nonsense"), "{}");
    }
}
