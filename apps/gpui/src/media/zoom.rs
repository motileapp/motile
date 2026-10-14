//! An image that fits its room until it is zoomed, as the Mac's `ZoomableImage` does: a double
//! click, a pinch, ⌘ and a mouse wheel, or ⌘+, ⌘−, ⌘0 for its real size and ⌘9 to fit again.
//! A double click and the keys zoom smoothly. Zoomed, it is moved by scrolling or by dragging
//! anywhere. The viewer shows its images with it, and the right panel an image file.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::pictures::{Picture, Pictures};
use crate::theme::{ControlSize, colors};
use crate::ui::Spinner;

const MAX_ZOOM: f32 = 8.;
/// The largest an image is decoded, on its longer side.
const MAX_PIXELS: u32 = 8192;
/// What a notch of a mouse wheel with ⌘ zooms by.
const WHEEL_ZOOM: f32 = 1.12;
/// How long a zoom by a double click or a key takes, as AppKit's animations do.
const ZOOM_FOR: Duration = Duration::from_millis(250);

type ClickedBeside = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct ZoomableImage {
    file: PathBuf,
    /// The room it leaves around it when it fits.
    margin: Size<f32>,
    radius: f32,
    /// It lies over a picture's darkness, as in the viewer.
    over_picture: bool,
    clicked_beside: Option<ClickedBeside>,
    /// The display's pixels to a point.
    pixels_per_point: f32,
    /// How much it is magnified, and how far it is moved from the middle.
    scale: Option<f32>,
    offset: Point<Pixels>,
    /// A zoom on its way: the scale and the offset it set out from, and when.
    zooming: Option<(f32, Point<Pixels>, Instant)>,
    dragging: Option<Point<Pixels>>,
    dragged: bool,
    /// A press began on it, beside the picture or on it, not on what lies over it.
    pressed: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Where the picture was drawn.
    picture_bounds: Rc<Cell<Bounds<Pixels>>>,
}

/// The image whose keys ⌘+, ⌘−, ⌘0 and ⌘9 are, when one has them.
#[derive(Default)]
struct ZoomKeys(Option<WeakEntity<ZoomableImage>>);

impl Global for ZoomKeys {}

impl ZoomableImage {
    pub fn new(file: &Path, margin: Size<f32>, radius: f32) -> Self {
        Self {
            file: file.to_path_buf(),
            margin,
            radius,
            over_picture: false,
            clicked_beside: None,
            pixels_per_point: 2.,
            scale: None,
            offset: Point::default(),
            zooming: None,
            dragging: None,
            dragged: false,
            pressed: false,
            bounds: Rc::default(),
            picture_bounds: Rc::default(),
        }
    }

    pub fn over_picture(mut self) -> Self {
        self.over_picture = true;
        self
    }

    pub fn on_clicked_beside(mut self, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.clicked_beside = Some(Rc::new(run));
        self
    }

    /// Makes ⌘+, ⌘−, ⌘0 and ⌘9 this image's, or with `None` no image's.
    pub fn take_keys(image: Option<&Entity<ZoomableImage>>, cx: &mut App) {
        cx.set_global(ZoomKeys(image.map(Entity::downgrade)));
    }

    /// Zooms the image whose keys they are, when the key is one of them. Whether it did.
    pub fn key(keystroke: &Keystroke, cx: &mut App) -> bool {
        let Some(image) = cx.try_global::<ZoomKeys>().and_then(|keys| keys.0.as_ref()?.upgrade()) else {
            return false;
        };
        image.update(cx, |image, cx| image.zoom_key(keystroke, cx))
    }

    /// ⌘+, ⌘−, ⌘0 and ⌘9. Whether the key was one of them.
    pub fn zoom_key(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) -> bool {
        if !keystroke.modifiers.secondary() {
            return false;
        }
        let scale = self.scale_now(cx);
        let target = match keystroke.key.as_str() {
            "=" | "+" => scale * 1.5,
            "-" => scale / 1.5,
            "0" => 1.,
            "9" => 0.,
            _ => return false,
        };
        self.zoom_smoothly(target, None, cx);
        true
    }

    /// The picture, and its size in points: its pixels at the display's scale.
    fn picture(&self, cx: &mut App) -> (Picture, Size<f32>) {
        let picture = Pictures::load_file(&format!("view:{}", self.file.display()), &self.file, MAX_PIXELS, cx);
        let Picture::Ready(shown) = &picture else { return (picture, size(0., 0.)) };
        let scale = self.pixels_per_point;
        let points = size(shown.width as f32 / scale, shown.height as f32 / scale);
        (picture, points)
    }

    /// The scale at which the image fits its room, never larger than its real size.
    fn fit(&self, points: Size<f32>) -> f32 {
        let room = self.bounds.get().size;
        if points.width <= 0. || points.height <= 0. {
            return 1.;
        }
        let across = (f32::from(room.width) - 2. * self.margin.width) / points.width;
        let down = (f32::from(room.height) - 2. * self.margin.height) / points.height;
        across.min(down).clamp(0.01, 1.)
    }

    fn scale_now(&self, cx: &mut App) -> f32 {
        let (_, points) = self.picture(cx);
        self.scale.unwrap_or(self.fit(points))
    }

    /// Zooms as a double click and the keys do, over a moment.
    fn zoom_smoothly(&mut self, scale: f32, around: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let (_, points) = self.picture(cx);
        let from = (self.shown_zoom(points), Instant::now());
        self.zoom_to(scale, around, cx);
        self.zooming = Some((from.0.0, from.0.1, from.1));
    }

    /// The scale and the offset drawn now, part of the way through a zoom.
    fn shown_zoom(&self, points: Size<f32>) -> (f32, Point<Pixels>) {
        let target = (self.scale.unwrap_or(self.fit(points)), self.offset);
        let Some((scale, offset, since)) = self.zooming else { return target };
        let progress = since.elapsed().as_secs_f32() / ZOOM_FOR.as_secs_f32();
        if progress >= 1. {
            return target;
        }
        let t = crate::ui::motion::ease_in_out(progress);
        (scale + (target.0 - scale) * t, offset + (target.1 - offset) * t)
    }

    fn on_picture(&self, position: Point<Pixels>) -> bool {
        self.picture_bounds.get().contains(&position)
    }

    fn zoom_to(&mut self, scale: f32, around: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        self.zooming = None;
        let (Picture::Ready(_), points) = self.picture(cx) else { return };
        let fit = self.fit(points);
        let old = self.scale.unwrap_or(fit);
        let new = scale.clamp(fit, MAX_ZOOM);
        // The point under the pointer stays where it is.
        if let Some(around) = around {
            let from_center = around - self.bounds.get().center() - self.offset;
            self.offset -= from_center * (new / old - 1.);
        }
        self.scale = Some(new);
        if (new - fit).abs() < 0.001 {
            self.offset = Point::default();
        }
        self.clamp_offset(points);
        cx.notify();
    }

    fn clamp_offset(&mut self, points: Size<f32>) {
        let room = self.bounds.get().size;
        let scale = self.scale.unwrap_or(self.fit(points));
        let spare_x = ((points.width * scale - f32::from(room.width)) / 2.).max(0.);
        let spare_y = ((points.height * scale - f32::from(room.height)) / 2.).max(0.);
        self.offset.x = self.offset.x.clamp(px(-spare_x), px(spare_x));
        self.offset.y = self.offset.y.clamp(px(-spare_y), px(spare_y));
    }

    /// A scale left from a larger room, smaller than fitting now, is let go of.
    fn refit(&mut self, points: Size<f32>) {
        let fit = self.fit(points);
        if self.scale.is_some_and(|scale| scale < fit) {
            self.scale = None;
            self.offset = Point::default();
        }
    }
}

impl Render for ZoomableImage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.pixels_per_point = window.scale_factor();
        let c = colors(cx);
        let quiet = if self.over_picture { c.muted_foreground_over_picture } else { c.muted_foreground };
        let (picture, points) = self.picture(cx);
        let measured = self.bounds.clone();
        let tracked = canvas(
            move |bounds, window, _| {
                if measured.get() != bounds {
                    measured.set(bounds);
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let content: AnyElement = match picture {
            Picture::Ready(shown) => {
                self.refit(points);
                let (scale, offset) = self.shown_zoom(points);
                match self.zooming {
                    Some((_, _, since)) if since.elapsed() < ZOOM_FOR => window.request_animation_frame(),
                    _ => self.zooming = None,
                }
                let room = self.bounds.get();
                let (width, height) = (points.width * scale, points.height * scale);
                let left = (room.size.width - px(width)) / 2. + offset.x;
                let top = (room.size.height - px(height)) / 2. + offset.y;
                self.picture_bounds.set(Bounds::new(room.origin + point(left, top), size(px(width), px(height))));
                let radius = px(self.radius);
                div()
                    .id("zoomed-image")
                    .absolute()
                    .left(left)
                    .top(top)
                    .w(px(width))
                    .h(px(height))
                    .rounded(radius)
                    .overflow_hidden()
                    .child(
                        img(ImageSource::Render(shown.image)).size_full().rounded(radius).object_fit(ObjectFit::Fill),
                    )
                    .into_any_element()
            }
            Picture::Failed(_) => div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .text_color(quiet)
                .child("This image couldn't be loaded.")
                .into_any_element(),
            Picture::Loading => div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .text_color(if self.over_picture { quiet } else { c.muted_stronger_foreground })
                .child(Spinner::new(ControlSize::Large.symbol()).render(cx))
                .into_any_element(),
        };
        div()
            .id("zoomable")
            .size_full()
            .relative()
            .overflow_hidden()
            .child(tracked)
            // A press anywhere drags the picture; a double click on it zooms, and a click beside it
            // that didn't drag says so.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.dragging = Some(event.position);
                    this.dragged = false;
                    this.pressed = true;
                    if event.click_count == 2 && this.on_picture(event.position) {
                        let (_, points) = this.picture(cx);
                        let fit = this.fit(points);
                        let zoomed = this.scale.unwrap_or(fit) > fit + 0.001;
                        let target = if zoomed { fit } else { (fit * 2.).max(1.) };
                        this.zoom_smoothly(target, Some(event.position), cx);
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let Some(from) = this.dragging else { return };
                if event.pressed_button != Some(MouseButton::Left) {
                    this.dragging = None;
                    return;
                }
                if event.position == from {
                    return;
                }
                let (_, points) = this.picture(cx);
                this.zooming = None;
                this.offset += event.position - from;
                this.dragging = Some(event.position);
                this.dragged = true;
                this.clamp_offset(points);
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &MouseUpEvent, window, cx| {
                    let beside = this.pressed && !this.dragged && !this.on_picture(event.position);
                    this.dragging = None;
                    this.pressed = false;
                    this.dragged = false;
                    if beside && let Some(clicked) = this.clicked_beside.clone() {
                        clicked(window, cx);
                    }
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                let (Picture::Ready(_), points) = this.picture(cx) else { return };
                // A mouse wheel with ⌘ zooms around the pointer; a trackpad moves the image.
                if let ScrollDelta::Lines(lines) = event.delta
                    && event.modifiers.secondary()
                {
                    if lines.y != 0. {
                        let scale = this.scale_now(cx);
                        this.zoom_to(scale * WHEEL_ZOOM.powf(lines.y), Some(event.position), cx);
                    }
                    return;
                }
                this.zooming = None;
                this.offset += event.delta.pixel_delta(px(20.));
                this.clamp_offset(points);
                cx.notify();
            }))
            .on_pinch(cx.listener(|this, event: &PinchEvent, _, cx| {
                let scale = this.scale_now(cx);
                this.zoom_to(scale * (1. + event.delta), Some(event.position), cx);
            }))
            .child(content)
    }
}
