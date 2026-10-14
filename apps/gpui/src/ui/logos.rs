//! The marks of Motile, the agents, GitHub and Google, the icon of a project, and the labels of a
//! server and of a pull request, as the Mac app's `Icons.swift` draws them.

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::{Agent, PullRequest};

use crate::models::{Project, Server};
use crate::theme::{Colors, colors};
use crate::ui::{OnClick, dims, icons, tooltip};

/// The colour an agent's logo and its series in a chart are drawn in.
pub fn agent_color(agent: Agent, c: &Colors) -> Hsla {
    match agent {
        Agent::Claude => c.claude,
        Agent::Codex => c.openai,
    }
}

/// An agent's logo, in its colour.
pub fn agent_icon(agent: Agent, size: f32, cx: &App) -> Svg {
    let path = match agent {
        Agent::Claude => "icons/claude.svg",
        Agent::Codex => "icons/openai.svg",
    };
    svg().path(path).size(px(size)).flex_shrink_0().text_color(agent_color(agent, colors(cx)))
}

pub fn github_mark(size: f32, cx: &App) -> Svg {
    svg().path("icons/github-mark.svg").size(px(size)).flex_shrink_0().text_color(colors(cx).foreground)
}

/// Google's "G" in its four colours, from Google's own mark on its 48-point grid.
pub fn google_mark(size: f32, cx: &App) -> Div {
    let c = colors(cx);
    let part = |name: &str, color: Hsla| {
        svg().path(SharedString::from(format!("icons/google-{name}.svg"))).absolute().size(px(size)).text_color(color)
    };
    div()
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .child(part("red", c.google_red))
        .child(part("blue", c.google_blue))
        .child(part("yellow", c.google_yellow))
        .child(part("green", c.google_green))
}

/// Motile's mark, as the app icon draws it.
pub fn motile_mark(size: f32, cx: &App) -> Svg {
    svg().path("icons/motile-mark.svg").size(px(size)).flex_shrink_0().text_color(colors(cx).emphasized_foreground)
}

/// A project's icon, or a folder when it has none; a server's "No project" shows a dashed
/// message.
pub fn project_icon(project: Option<&Project>, size: f32, cx: &App) -> AnyElement {
    let path = project.and_then(|project| project.icon_path.clone());
    let Some(path) = path else {
        let symbol = project.map_or("folder", Project::symbol);
        return div()
            .size(px(size))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .text_color(colors(cx).muted_foreground)
            .child(icons::symbol(symbol, size * 0.78))
            .into_any_element();
    };
    let folder = colors(cx).muted_foreground;
    let file = std::path::PathBuf::from(path);
    let filled = file.clone();
    let source = ImageSource::Custom(std::sync::Arc::new(move |window, cx| {
        window.use_asset::<FilledIcon>(&filled, cx).map(|filled| filled.map_err(ImageCacheError::Asset))
    }));
    // The folder stands in while the icon is read, as the Mac app's does; a picture it can't
    // trim is drawn as it is.
    img(source)
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(size * 0.22))
        .object_fit(ObjectFit::Contain)
        .with_loading(move || {
            div()
                .size(px(size))
                .flex()
                .items_center()
                .justify_center()
                .text_color(folder)
                .child(icons::symbol("folder", size * 0.78))
                .into_any_element()
        })
        .with_fallback(move || img(file.clone()).size(px(size)).object_fit(ObjectFit::Contain).into_any_element())
        .into_any_element()
}

/// A project's icon as a square its picture fills: drawn at one size and without the empty
/// margin some icons come with, so that every icon is as large as the next. Read once, off the
/// main thread.
enum FilledIcon {}

impl Asset for FilledIcon {
    type Source = std::path::PathBuf;
    type Output = Result<std::sync::Arc<RenderImage>, SharedString>;

    fn load(file: Self::Source, cx: &mut App) -> impl std::future::Future<Output = Self::Output> + Send + 'static {
        let svg = cx.svg_renderer();
        async move { filled_icon(&file, &svg).ok_or_else(|| SharedString::from("the icon couldn't be trimmed")) }
    }
}

/// How large the picture is drawn to find its margin, and how large the square it fills is.
const DRAWN: u32 = 256;
const FILLED: u32 = 128;

fn filled_icon(file: &std::path::Path, svg: &SvgRenderer) -> Option<std::sync::Arc<RenderImage>> {
    let bytes = std::fs::read(file).ok()?;
    let picture = if is_svg(&bytes) {
        let natural = svg.render_single_frame(&bytes, 1.).ok()?.size(0);
        let longest = i32::from(natural.width).max(i32::from(natural.height)).max(1) as f32;
        svg.render_single_frame(&bytes, DRAWN as f32 / longest).ok()?
    } else {
        let format = match image::guess_format(&bytes).ok()? {
            image::ImageFormat::Png => ImageFormat::Png,
            image::ImageFormat::Jpeg => ImageFormat::Jpeg,
            image::ImageFormat::WebP => ImageFormat::Webp,
            image::ImageFormat::Gif => ImageFormat::Gif,
            image::ImageFormat::Bmp => ImageFormat::Bmp,
            image::ImageFormat::Tiff => ImageFormat::Tiff,
            image::ImageFormat::Ico => ImageFormat::Ico,
            _ => return None,
        };
        Image::from_bytes(format, bytes).to_image_data(svg.clone()).ok()?
    };
    let frame = picture.as_bytes(0)?;
    let size = picture.size(0);
    let (width, height) = (i32::from(size.width) as u32, i32::from(size.height) as u32);
    let source = image::RgbaImage::from_raw(width, height, frame.to_vec())?;
    Some(std::sync::Arc::new(RenderImage::new(vec![image::Frame::new(filled(&source)?)])))
}

/// The picture without its empty margin, scaled to fill a square.
fn filled(picture: &image::RgbaImage) -> Option<image::RgbaImage> {
    let canvas = fitted(picture, DRAWN)?;
    let (left, top, right, bottom) = opaque_bounds(&canvas)?;
    let cropped = image::imageops::crop_imm(&canvas, left, top, right - left + 1, bottom - top + 1).to_image();
    fitted(&cropped, FILLED)
}

/// The picture scaled to fit a transparent square of `side`, in its middle.
fn fitted(picture: &image::RgbaImage, side: u32) -> Option<image::RgbaImage> {
    use image::imageops::{FilterType, overlay, resize};

    let (width, height) = picture.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let fit = (side as f32 / width as f32).min(side as f32 / height as f32);
    let (to_width, to_height) =
        (((width as f32 * fit).round() as u32).max(1), ((height as f32 * fit).round() as u32).max(1));
    let scaled = resize(picture, to_width, to_height, FilterType::Lanczos3);
    let mut square = image::RgbaImage::new(side, side);
    overlay(&mut square, &scaled, i64::from((side - to_width) / 2), i64::from((side - to_height) / 2));
    Some(square)
}

/// The left, top, right and bottom of what isn't transparent, as the Mac app finds them.
fn opaque_bounds(picture: &image::RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for (x, y, pixel) in picture.enumerate_pixels() {
        if pixel[3] <= 24 {
            continue;
        }
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((left, top, right, bottom)) => (left.min(x), top.min(y), right.max(x), bottom.max(y)),
        });
    }
    bounds
}

fn is_svg(bytes: &[u8]) -> bool {
    let start = &bytes[..bytes.len().min(512)];
    String::from_utf8_lossy(start).contains("<svg")
}

/// The server a thread or project is on, for when there is more than one, by its short name.
/// With `shows_state` its icon is tinted while it isn't connected.
pub fn server_label(server: &Server, size: f32, shows_state: bool, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let muted = c.muted_stronger_foreground;
    let tint = if shows_state { server.state_tint(c).unwrap_or(muted) } else { muted };
    div()
        .id(SharedString::from(format!("server-label-{}", server.id)))
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(3.))
        .text_color(muted)
        // Lucide's server fills more of its square than the icons beside it.
        .child(icons::symbol("server", size - 1.).text_color(tint))
        .child(div().text_size(px(size)).truncate().child(server.short_name.clone()))
        .tooltip(tooltip(format!("On {}", server.name)))
}

/// The symbol of what became of a pull request.
pub fn pull_request_symbol(pull_request: &PullRequest) -> &'static str {
    if pull_request.merged {
        return "git-merge";
    }
    if pull_request.closed {
        return "git-pull-request-closed";
    }
    if pull_request.draft { "git-pull-request-draft" } else { "git-pull-request" }
}

/// The colour of what became of a pull request.
pub fn pull_request_color(pull_request: &PullRequest, c: &Colors) -> Hsla {
    if pull_request.merged {
        return c.merged;
    }
    if pull_request.closed {
        return c.destructive;
    }
    if pull_request.draft { c.muted_foreground } else { c.success }
}

/// A thread's pull request: what became of it, and its number. In the colour of what became of
/// it, or as quiet as the row it is in. When it has a click, its number underlines under the
/// pointer.
#[derive(IntoElement)]
pub struct PullRequestLabel {
    id: ElementId,
    pull_request: PullRequest,
    colored: bool,
    on_click: Option<OnClick>,
}

impl PullRequestLabel {
    pub fn new(id: impl Into<ElementId>, pull_request: &PullRequest) -> Self {
        Self { id: id.into(), pull_request: pull_request.clone(), colored: true, on_click: None }
    }

    pub fn colored(mut self, colored: bool) -> Self {
        self.colored = colored;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(std::rc::Rc::new(handler));
        self
    }
}

impl RenderOnce for PullRequestLabel {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let color = if self.colored { pull_request_color(&self.pull_request, c) } else { c.muted_stronger_foreground };
        let group: SharedString = format!("pull-request-label-{:?}", self.id).into();
        let clickable = self.on_click.is_some();
        let label = div()
            .id(self.id)
            .group(group.clone())
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(2.))
            .text_color(color)
            .child(icons::symbol(pull_request_symbol(&self.pull_request), 11.))
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .font_features(crate::ui::tabular_numbers())
                    .when(clickable, |number| number.group_hover(group, |number| number.text_decoration_1()))
                    .child(self.pull_request.number.to_string()),
            )
            .tooltip(tooltip(self.pull_request.title.clone()));
        let Some(on_click) = self.on_click else { return label };
        dims(label, cx).on_click(move |event, window, cx| {
            cx.stop_propagation();
            on_click(event, window, cx)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{FILLED, filled};

    #[test]
    fn an_icon_with_a_margin_fills_its_square() {
        let mut padded = image::RgbaImage::new(100, 100);
        for (x, y, pixel) in padded.enumerate_pixels_mut() {
            if (20..80).contains(&x) && (30..70).contains(&y) {
                *pixel = image::Rgba([255, 0, 0, 255]);
            }
        }
        let square = filled(&padded).unwrap();
        assert_eq!(square.dimensions(), (FILLED, FILLED));
        assert_eq!(square.get_pixel(2, FILLED / 2)[3], 255);
        assert_eq!(square.get_pixel(FILLED - 3, FILLED / 2)[3], 255);
        assert_eq!(square.get_pixel(FILLED / 2, 2)[3], 0);
        assert!(filled(&image::RgbaImage::new(10, 10)).is_none());
    }
}
