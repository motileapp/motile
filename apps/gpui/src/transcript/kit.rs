//! The small parts the transcript's rows are drawn with, as the Mac's `Kit.swift` has them: the
//! button of words inside a row, the ring that says how much has arrived, a dotted outline, the
//! band of light that sweeps over what works, and the fade of what a row grew by.

use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme::{ControlSize, Opacity, colors};
use crate::ui::{OnClick, edges, tooltip};

/// A button of words inside a row. All of its frame takes the click, and what lights up under
/// the pointer is inset from it by `insets`, so buttons that touch each other and the row's edge
/// look apart. A bordered one is drawn as a regular button is, with a line around it, and a
/// primary one is filled with the primary colour.
#[derive(IntoElement)]
pub struct RowButton {
    id: ElementId,
    title: SharedString,
    help: SharedString,
    radius: f32,
    bordered: bool,
    primary: bool,
    hover: Option<Hsla>,
    rest: Option<Hsla>,
    grows: bool,
    insets: Edges<f32>,
    height: f32,
    on_click: Option<OnClick>,
}

impl RowButton {
    pub const METRICS: ControlSize = ControlSize::Regular;
    /// The room between the words and the light's sides, without a border.
    pub const PADDING: f32 = 8.;

    pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>, help: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            help: help.into(),
            radius: Self::METRICS.radius(),
            bordered: false,
            primary: false,
            hover: None,
            rest: None,
            grows: false,
            insets: edges(0., 0., 0., 0.),
            height: Self::METRICS.height(),
            on_click: None,
        }
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    pub fn bordered(mut self) -> Self {
        self.bordered = true;
        self
    }

    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    /// What lights it, a step above the surface it lies on.
    pub fn hover(mut self, hover: Hsla) -> Self {
        self.hover = Some(hover);
        self
    }

    /// What fills it unlit.
    pub fn rest(mut self, rest: Hsla) -> Self {
        self.rest = Some(rest);
        self
    }

    /// It takes its share of the row's width, beside the buttons that do too.
    pub fn grows(mut self) -> Self {
        self.grows = true;
        self
    }

    pub fn insets(mut self, insets: Edges<f32>) -> Self {
        self.insets = insets;
        self
    }

    /// The height of its whole frame, the insets with it.
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for RowButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let hovered = crate::ui::hover_state(&self.id, window, cx);
        let lit = *hovered.read(cx);
        let c = colors(cx);
        let insets = self.insets;
        let bordered = self.bordered || self.primary;
        let padding = if bordered { Self::METRICS.padding() } else { Self::PADDING };
        let (fill, text) = if self.primary {
            let fill = if lit { c.at(c.primary, Opacity::Lit) } else { c.primary };
            (Some(fill), c.primary_foreground)
        } else if lit {
            (Some(self.hover.unwrap_or(c.background_secondary)), c.foreground)
        } else {
            (self.rest, if bordered { c.foreground } else { c.muted_foreground })
        };
        let stroked = self.bordered && !self.primary;
        div()
            .id(self.id)
            .flex_shrink_0()
            .when(self.grows, |button| button.flex_1().min_w_0())
            .h(px(self.height))
            .pt(px(insets.top))
            .pl(px(insets.left))
            .pb(px(insets.bottom))
            .pr(px(insets.right))
            .cursor_default()
            .child(
                div()
                    .h_full()
                    .when(self.grows, |light| light.w_full())
                    .px(px(padding))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.radius))
                    .when(stroked, |light| light.border_1().border_color(c.border).px(px(padding - 1.)))
                    .when_some(fill, |light, fill| light.bg(fill))
                    .text_size(px(Self::METRICS.text_size()))
                    .when(bordered, |light| light.font_weight(FontWeight::MEDIUM))
                    .text_color(text)
                    .whitespace_nowrap()
                    .child(self.title),
            )
            .tooltip(tooltip(self.help))
            .on_hover(crate::ui::track_hover(&hovered))
            .when_some(self.on_click, |button, on_click| {
                button.on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_click(event, window, cx)
                })
            })
    }
}

/// How much of something has arrived, as a ring that fills clockwise from the top: a 2 point line,
/// its track in `track` and what has arrived in `arc`.
pub fn progress_ring(fraction: f32, track: Hsla, arc: Hsla) -> impl IntoElement {
    const LINE: f32 = 2.;
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let side = f32::from(bounds.size.width.min(bounds.size.height)) - LINE;
            let center = bounds.center();
            let radius = side / 2.;
            let at = |angle: f32| point(center.x + px(radius * angle.cos()), center.y + px(radius * angle.sin()));
            let mut circle = PathBuilder::stroke(px(LINE));
            circle.move_to(at(-PI / 2.));
            circle.arc_to(point(px(radius), px(radius)), px(0.), false, true, at(PI / 2.));
            circle.arc_to(point(px(radius), px(radius)), px(0.), false, true, at(-PI / 2.));
            if let Ok(path) = circle.build() {
                window.paint_path(path, track);
            }
            let fraction = fraction.clamp(0., 1.);
            if fraction <= 0. {
                return;
            }
            let mut filled = PathBuilder::stroke(px(LINE));
            filled.move_to(at(-PI / 2.));
            let end = -PI / 2. + 2. * PI * fraction.min(0.9999);
            filled.arc_to(point(px(radius), px(radius)), px(0.), fraction > 0.5, true, at(end));
            if let Ok(path) = filled.build() {
                window.paint_path(path, arc);
            }
        },
    )
    .size_full()
}

/// A dashed line around the element's bounds, with round corners: `dash` is a dash's length and
/// the gap's.
pub fn dashed_outline(radius: f32, dash: [f32; 2], color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let inset = px(0.5);
            let (left, top) = (bounds.left() + inset, bounds.top() + inset);
            let (right, bottom) = (bounds.right() - inset, bounds.bottom() - inset);
            let corner = px(radius - 0.5).min((right - left) / 2.).min((bottom - top) / 2.).max(px(0.));
            let radii = point(corner, corner);
            let mut outline = PathBuilder::stroke(px(1.)).dash_array(&dash.map(px));
            outline.move_to(point(left + corner, top));
            outline.line_to(point(right - corner, top));
            outline.arc_to(radii, px(0.), false, true, point(right, top + corner));
            outline.line_to(point(right, bottom - corner));
            outline.arc_to(radii, px(0.), false, true, point(right - corner, bottom));
            outline.line_to(point(left + corner, bottom));
            outline.arc_to(radii, px(0.), false, true, point(left, bottom - corner));
            outline.line_to(point(left, top + corner));
            outline.arc_to(radii, px(0.), false, true, point(left + corner, top));
            if let Ok(path) = outline.build() {
                window.paint_path(path, color);
            }
        },
    )
    .absolute()
    .inset_0()
}

/// How far code that is wider than its box has been moved, and the way the gesture under way set
/// out in.
#[derive(Default)]
struct SidewaysState {
    offset: f32,
    content: f32,
    overflow: f32,
    sideways: Option<bool>,
}

/// Code wider than the column, moved sideways under the pointer. A gesture keeps the way it set
/// out, as it does in a scroll view, so one that scrolls the transcript never drags the code along.
#[derive(IntoElement)]
pub struct Sideways {
    id: ElementId,
    child: AnyElement,
}

impl Sideways {
    pub fn new(id: impl Into<ElementId>, child: impl IntoElement) -> Self {
        Self { id: id.into(), child: child.into_any_element() }
    }
}

impl RenderOnce for Sideways {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| SidewaysState::default());
        let offset = state.read(cx).offset;
        let (outer, inner, scrolled) = (state.clone(), state.clone(), state);
        div()
            .w_full()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .child(div().flex_none().min_w_full().ml(px(-offset)).child(self.child))
                    .on_children_prepainted(move |bounds, _, cx| {
                        let Some(content) = bounds.first() else { return };
                        let width = f32::from(content.size.width);
                        inner.update(cx, |state, _| state.content = width);
                    }),
            )
            .on_children_prepainted(move |bounds, _, cx| {
                let Some(row) = bounds.first() else { return };
                let width = f32::from(row.size.width);
                outer.update(cx, |state, cx| {
                    let overflow = (state.content - width).max(0.);
                    if overflow != state.overflow {
                        state.overflow = overflow;
                        if state.offset > overflow {
                            state.offset = overflow;
                            cx.notify();
                        }
                    }
                });
            })
            .id(self.id)
            .on_scroll_wheel(move |event, _, cx| {
                let delta = event.delta.pixel_delta(px(18.));
                let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
                let sideways = scrolled.update(cx, |state, _| {
                    let sideways = dx.abs() > dy.abs();
                    if matches!(event.delta, ScrollDelta::Lines(_)) {
                        return sideways;
                    }
                    if event.touch_phase == TouchPhase::Started {
                        state.sideways = None;
                    }
                    if state.sideways.is_none() && (dx != 0. || dy != 0.) {
                        state.sideways = Some(sideways);
                    }
                    state.sideways.unwrap_or(false)
                });
                if !sideways || scrolled.read(cx).overflow <= 0. {
                    return;
                }
                cx.stop_propagation();
                scrolled.update(cx, |state, cx| {
                    state.offset = (state.offset - dx).clamp(0., state.overflow);
                    cx.notify();
                });
            })
    }
}

/// How wide the band of a shimmer is, and how long it takes to sweep across.
const SHIMMER_BAND: f32 = 72.;
const SHIMMER_PERIOD: Duration = Duration::from_millis(2200);
/// How many strips the band is drawn in, each at the light of its middle.
const SHIMMER_STRIPS: usize = 18;

/// A label with a bright copy of it laid over, seen only through a soft band that sweeps across
/// it, as the Mac's `ShimmerLabel` is. Every band sets out on the same beat, so labels move together.
#[derive(IntoElement)]
pub struct Shimmer {
    id: ElementId,
    text: SharedString,
    runs: Vec<TextRun>,
    bright: Hsla,
}

impl Shimmer {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>, runs: Vec<TextRun>, bright: Hsla) -> Self {
        Self { id: id.into(), text: text.into(), runs, bright }
    }
}

impl RenderOnce for Shimmer {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let line = TailLine::new(self.text, self.runs);
        if crate::media::native::reduces_motion() {
            return line.into_any_element();
        }
        let c = colors(cx);
        let (edge, middle) = (c.opacity(Opacity::ShimmerBandEdge), c.opacity(Opacity::ShimmerBandMiddle));
        ShimmerGlyphs { line, bright: self.bright, alphas: [0., edge, middle, 1., middle, edge, 0.], phase: 0. }
            .with_animation(self.id, Animation::new(SHIMMER_PERIOD).repeat_synced(), |mut glyphs, phase| {
                glyphs.phase = phase;
                glyphs
            })
            .into_any_element()
    }
}

/// A shimmer band's light `across` its width (0 to 1), its `alphas` at the Mac's gradient stops.
fn band_alpha(alphas: &[f32; 7], across: f32) -> f32 {
    let Some(index) = ShimmerGlyphs::STOPS.windows(2).position(|pair| across <= pair[1]) else { return 0. };
    let (from, to) = (ShimmerGlyphs::STOPS[index], ShimmerGlyphs::STOPS[index + 1]);
    let t = ((across - from) / (to - from)).clamp(0., 1.);
    alphas[index] + (alphas[index + 1] - alphas[index]) * t
}

struct ShimmerGlyphs {
    line: TailLine,
    bright: Hsla,
    /// The band's light at 0, .15, .35, .5, .65, .85 and 1 of its width, as the Mac's gradient.
    alphas: [f32; 7],
    phase: f32,
}

impl ShimmerGlyphs {
    const STOPS: [f32; 7] = [0., 0.15, 0.35, 0.5, 0.65, 0.85, 1.];

    fn alpha_at(&self, across: f32) -> f32 {
        band_alpha(&self.alphas, across)
    }

    fn paint_band(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        let Some((line, line_height)) = self.line.shaped.borrow().clone() else { return };
        let travel = f32::from(line.width.min(bounds.size.width)) + SHIMMER_BAND;
        let left = f32::from(bounds.left()) - SHIMMER_BAND + self.phase * travel;
        let strip = SHIMMER_BAND / SHIMMER_STRIPS as f32;
        let baseline = bounds.top() + (line_height - line.ascent - line.descent) / 2. + line.ascent;
        for index in 0..SHIMMER_STRIPS {
            let alpha = self.alpha_at((index as f32 + 0.5) / SHIMMER_STRIPS as f32);
            if alpha <= 0. {
                continue;
            }
            let mask =
                Bounds::new(point(px(left + strip * index as f32), bounds.top()), size(px(strip), bounds.size.height));
            let mask = mask.intersect(&bounds);
            if mask.size.width <= px(0.) {
                continue;
            }
            let color = self.bright.opacity(self.bright.a * alpha);
            window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                for run in &line.runs {
                    for glyph in &run.glyphs {
                        let x = bounds.left() + glyph.position.x;
                        if glyph.is_emoji || x > mask.right() || x + line.font_size < mask.left() {
                            continue;
                        }
                        let origin = point(x, baseline + glyph.position.y);
                        let _ = window.paint_glyph(origin, run.font_id, glyph.id, line.font_size, color);
                    }
                }
            });
        }
    }
}

impl IntoElement for ShimmerGlyphs {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for ShimmerGlyphs {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.line.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.line.paint(id, inspector_id, bounds, state, prepaint, window, cx);
        self.paint_band(bounds, window);
    }
}

/// One line of text in runs of different fonts, cut at its end with an ellipsis where it doesn't
/// fit, as the Mac's tail truncation does. GPUI's own truncation measures every character in the
/// element's font, so a line with code in it would overflow its box and lose its end unseen.
pub struct TailLine {
    text: SharedString,
    runs: Vec<TextRun>,
    /// The line as it was last fitted, and its height.
    shaped: Rc<RefCell<Option<(ShapedLine, Pixels)>>>,
}

impl TailLine {
    pub fn new(text: impl Into<SharedString>, runs: Vec<TextRun>) -> Self {
        Self { text: text.into(), runs, shaped: Rc::default() }
    }
}

/// The line shaped whole, or cut to fit in `width` with an ellipsis in the font it is cut in.
fn fitted(
    text: &SharedString,
    runs: &[TextRun],
    font_size: Pixels,
    width: Option<Pixels>,
    window: &Window,
) -> ShapedLine {
    const ELLIPSIS: &str = "…";
    let system = window.text_system();
    let whole = system.shape_line(text.clone(), font_size, runs, None);
    let Some(width) = width.filter(|width| whole.width > *width) else { return whole };
    let style_at = |index: usize| {
        let mut end = 0;
        runs.iter().find(|run| {
            end += run.len;
            index < end
        })
    };
    let Some(style) = style_at(whole.index_for_x(width).unwrap_or(0)).or(runs.last()) else { return whole };
    let ellipsis = TextRun { len: ELLIPSIS.len(), ..style.clone() };
    let ellipsis_width = system.shape_line(ELLIPSIS.into(), font_size, std::slice::from_ref(&ellipsis), None).width;
    let cut = whole.index_for_x(width - ellipsis_width).unwrap_or(0);
    let kept = text[..cut].trim_end();
    let mut left = kept.len();
    let mut cut_runs: Vec<TextRun> = runs
        .iter()
        .map_while(|run| {
            let len = run.len.min(left);
            left -= len;
            (len > 0).then(|| TextRun { len, ..run.clone() })
        })
        .collect();
    cut_runs.push(ellipsis);
    system.shape_line(format!("{kept}{ELLIPSIS}").into(), font_size, &cut_runs, None)
}

impl IntoElement for TailLine {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for TailLine {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.pixel_snap(style.line_height.to_pixels(font_size.into(), window.rem_size()));
        let (text, runs, shaped) = (self.text.clone(), self.runs.clone(), self.shaped.clone());
        let layout = window.request_measured_layout(Style::default(), move |known, available, window, _| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(width) => Some(width),
                _ => None,
            });
            let line = fitted(&text, &runs, font_size, width, window);
            let size = size(line.width.ceil(), line_height);
            *shaped.borrow_mut() = Some((line, line_height));
            size
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some((line, line_height)) = self.shaped.borrow().clone() else { return };
        let _ = line.paint(bounds.origin, line_height, TextAlign::Left, None, window, cx);
    }
}

/// Fades in whatever part of a row lies under the height it had, once it grew, as the Mac's
/// `GrowthFade` does: what is under is covered with the background, which clears.
pub struct GrowthFade {
    id: ElementId,
    /// When the row last grew, while it streams.
    grown: Option<Instant>,
    background: Hsla,
    child: AnyElement,
}

#[derive(Default)]
struct Growth {
    size: Size<Pixels>,
    seen: Option<Instant>,
    fade: Option<(Pixels, Instant)>,
}

impl GrowthFade {
    pub fn new(id: impl Into<ElementId>, grown: Option<Instant>, background: Hsla, child: impl IntoElement) -> Self {
        Self { id: id.into(), grown, background, child: child.into_any_element() }
    }
}

impl IntoElement for GrowthFade {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for GrowthFade {
    type RequestLayoutState = ();
    type PrepaintState = Option<(Pixels, f32)>;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
        let grown = self.grown;
        window.with_element_state(id.expect("GrowthFade has an id"), |growth: Option<Growth>, window| {
            let mut growth = growth.unwrap_or_default();
            if let Some(at) = grown
                && growth.seen != Some(at)
            {
                let before = growth.size;
                if before.width == bounds.size.width
                    && before.height > px(0.)
                    && bounds.size.height > before.height + px(1.)
                {
                    growth.fade = Some((before.height, at));
                }
                growth.seen = Some(at);
            }
            growth.size = bounds.size;
            let covered = growth.fade.and_then(|(from, at)| {
                let progress = at.elapsed().as_secs_f32() / super::model::FADE.as_secs_f32();
                (progress < 1.).then(|| (from, 1. - crate::ui::ease_out(progress)))
            });
            if covered.is_some() {
                window.request_animation_frame();
            } else {
                growth.fade = None;
            }
            (covered, growth)
        })
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        covered: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
        let Some((from, cover)) = *covered else { return };
        let below = Bounds::from_corners(point(bounds.left(), bounds.top() + from), bounds.bottom_right());
        window.paint_quad(fill(below, self.background.opacity(self.background.a * cover)));
    }
}

/// How long the band of a box's shimmer takes to sweep across it, and how much of the box it is.
const BOX_SHIMMER_PERIOD: Duration = Duration::from_millis(1400);
const BOX_SHIMMER_BAND: f32 = 0.6;

/// What waits for a picture: a soft band of `color` that sweeps across the box it fills, from
/// before its leading edge to past its trailing one, held still under reduced motion.
#[derive(IntoElement)]
pub struct ShimmerBox {
    id: ElementId,
    color: Hsla,
    radius: f32,
}

impl ShimmerBox {
    pub fn new(id: impl Into<ElementId>, color: Hsla) -> Self {
        Self { id: id.into(), color, radius: 0. }
    }

    /// The corners of the box it fills, which the band keeps inside.
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

/// How far a box's rounded corner of `radius` lies inside its top and bottom `x` from its side.
fn corner_inset(x: f32, radius: f32) -> f32 {
    if x >= radius {
        return 0.;
    }
    let across = radius - x.max(0.);
    radius - (radius * radius - across * across).max(0.).sqrt()
}

impl RenderOnce for ShimmerBox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let c = colors(cx);
        let (edge, middle) = (c.opacity(Opacity::ShimmerBandEdge), c.opacity(Opacity::ShimmerBandMiddle));
        let alphas = [0., edge, middle, 1., middle, edge, 0.];
        let (color, radius) = (self.color, self.radius);
        let band = move |phase: f32| {
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let width = f32::from(bounds.size.width);
                    let band = width * BOX_SHIMMER_BAND;
                    let left = -band + (width + band) * phase;
                    let strips = ((band / 2.).ceil() as usize).max(SHIMMER_STRIPS);
                    let strip = band / strips as f32;
                    for index in 0..strips {
                        let alpha = band_alpha(&alphas, (index as f32 + 0.5) / strips as f32);
                        let x = (left + index as f32 * strip).max(0.);
                        let right = (left + (index + 1) as f32 * strip).min(width);
                        if right <= x {
                            continue;
                        }
                        let middle = (x + right) / 2.;
                        let radius = radius.min(width / 2.).min(f32::from(bounds.size.height) / 2.);
                        let inset = corner_inset(middle.min(width - middle), radius);
                        let height = f32::from(bounds.size.height) - 2. * inset;
                        if height <= 0. {
                            continue;
                        }
                        let quad =
                            Bounds::new(bounds.origin + point(px(x), px(inset)), size(px(right - x), px(height)));
                        window.paint_quad(fill(quad, color.opacity(color.a * alpha)));
                    }
                },
            )
            .absolute()
            .inset_0()
        };
        if crate::media::native::reduces_motion() {
            return band(0.5).into_any_element();
        }
        div()
            .absolute()
            .inset_0()
            .child(band(0.))
            .with_animation(self.id, Animation::new(BOX_SHIMMER_PERIOD).repeat(), move |_, phase| {
                div().absolute().inset_0().child(band(phase))
            })
            .into_any_element()
    }
}

/// Paints the corners of `bounds` outside a rounding of `radius` in `color`.
pub fn paint_corners(bounds: Bounds<Pixels>, radius: f32, color: Hsla, window: &mut Window) {
    let r = px(radius.min(f32::from(bounds.size.width.min(bounds.size.height)) / 2.));
    let bend = r * (1. - 0.5523);
    let corners = [
        (bounds.origin, 1., 1.),
        (bounds.top_right(), -1., 1.),
        (bounds.bottom_left(), 1., -1.),
        (bounds.bottom_right(), -1., -1.),
    ];
    for (corner, x, y) in corners {
        let mut path = PathBuilder::fill();
        path.move_to(corner);
        path.line_to(point(corner.x + r * x, corner.y));
        path.cubic_bezier_to(
            point(corner.x, corner.y + r * y),
            point(corner.x + bend * x, corner.y),
            point(corner.x, corner.y + bend * y),
        );
        path.close();
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}
