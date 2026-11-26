//! A message thrown in from the composer as it is sent, as Messages throws one: a copy of its
//! bubble starts around the text where it was written, takes its own shape and flies on springs
//! to its row, which stays hidden until it lands.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::RowKind;

use super::model::RowModel;
use super::rows::{self, RowContext};

/// The springs and curves Messages throws with.
const TRAVEL: Spring = Spring { mass: 1., stiffness: 141.759, damping: 17.3503 };
const LIFT: f32 = 0.055;
const SHRINK: Spring = Spring { mass: 2., stiffness: 310., damping: 38. };
const REGROW: Spring = Spring { mass: 2., stiffness: 320., damping: 38. };
const REGROW_AFTER: f32 = 0.185;
const RESHAPE: CubicCurve = CubicCurve { x1: 0.542, y1: 0., x2: 0.58, y2: 1. };
const RESHAPE_TIME: f32 = 0.2;
const DURATION: f32 = 1.;
/// The most a frame moves it on, so that a moment the main thread is held doesn't skip any of it.
const LONGEST_FRAME: f32 = 1. / 30.;
/// Scales are drawn in these steps, so that the glyphs they rasterize are few.
const SCALE_STEPS: f32 = 256.;
/// The composer's placeholder comes back this long after a throw, over `PLACEHOLDER_FADE`, so
/// that it doesn't show under the copy's text before its bubble has filled in.
const PLACEHOLDER_AFTER: f32 = 0.15;
const PLACEHOLDER_FADE: f32 = 0.2;

/// A spring as Core Animation runs one from rest: how far along it is `time` seconds in.
#[derive(Clone, Copy)]
pub struct Spring {
    mass: f32,
    stiffness: f32,
    damping: f32,
}

impl Spring {
    pub fn progress(&self, time: f32) -> f32 {
        if time <= 0. {
            return 0.;
        }
        let natural = (self.stiffness / self.mass).sqrt();
        let ratio = self.damping / (2. * (self.stiffness * self.mass).sqrt());
        if ratio >= 1. {
            return 1. - (-natural * time).exp() * (1. + natural * time);
        }
        let damped = natural * (1. - ratio * ratio).sqrt();
        let decay = (-ratio * natural * time).exp();
        1. - decay * ((damped * time).cos() + ratio * natural / damped * (damped * time).sin())
    }
}

/// A timing curve as `CAMediaTimingFunction(controlPoints:)` draws it.
#[derive(Clone, Copy)]
pub struct CubicCurve {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl CubicCurve {
    pub fn value(&self, x: f32) -> f32 {
        if x <= 0. {
            return 0.;
        }
        if x >= 1. {
            return 1.;
        }
        let (mut low, mut high) = (0., 1.);
        for _ in 0..24 {
            let middle = (low + high) / 2.;
            if bezier(middle, self.x1, self.x2) < x {
                low = middle;
            } else {
                high = middle;
            }
        }
        bezier((low + high) / 2., self.y1, self.y2)
    }
}

fn bezier(t: f32, a: f32, b: f32) -> f32 {
    let u = 1. - t;
    3. * u * u * t * a + 3. * u * t * t * b + t * t * t
}

/// Where each attachment's tile or file is in the window, by its index among the message's.
type Tiles = Vec<Option<Bounds<Pixels>>>;

/// What the composer last drew: what is written, where its first line's letters start in the
/// window, how wide its lines are and how they are set, and where each attachment's tile is.
#[derive(Clone, Default)]
pub struct Written {
    pub text: String,
    pub origin: Point<Pixels>,
    pub width: Pixels,
    pub font: Option<Font>,
    pub size: Pixels,
    pub line: Pixels,
    /// How far the letters are from the top and bottom of their lines.
    pub trim: Pixels,
    pub tiles: Tiles,
}

thread_local! {
    static WRITTEN: RefCell<Written> = RefCell::default();
    static THROWN_AT: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Has the composer's placeholder come back after a throw from it, which starts now.
pub fn fade_placeholder_in() {
    THROWN_AT.set(Some(Instant::now()));
}

/// How much of the composer's placeholder shows while it comes back after a throw; none once it
/// is all back.
pub fn placeholder_shown() -> Option<f32> {
    let elapsed = THROWN_AT.get()?.elapsed().as_secs_f32();
    if elapsed >= PLACEHOLDER_AFTER + PLACEHOLDER_FADE {
        THROWN_AT.set(None);
        return None;
    }
    Some(((elapsed - PLACEHOLDER_AFTER) / PLACEHOLDER_FADE).clamp(0., 1.))
}

/// Keeps what the composer drew, its tiles for `attachments` attachments as they were.
pub fn record_written(written: Written, attachments: usize) {
    WRITTEN.with_borrow_mut(|kept| {
        let mut tiles = std::mem::take(&mut kept.tiles);
        tiles.resize(attachments, None);
        *kept = Written { tiles, ..written };
    });
}

/// Keeps where the composer drew the tile of the attachment at `index`.
pub fn record_tile(index: usize, bounds: Bounds<Pixels>) {
    WRITTEN.with_borrow_mut(|kept| {
        if kept.tiles.len() <= index {
            kept.tiles.resize(index + 1, None);
        }
        kept.tiles[index] = Some(bounds);
    });
}

/// Where the composer shows the message `text` was sent with, while it still shows it: its
/// letters, unless there are none, and its files' tiles, in the window.
fn written(text: &str, window: &Window) -> Option<(Option<Bounds<Pixels>>, Tiles)> {
    let kept = WRITTEN.with_borrow(Clone::clone);
    let shown = kept.text.trim();
    if shown != text.trim() {
        return None;
    }
    if shown.is_empty() {
        return kept.tiles.iter().any(Option::is_some).then_some((None, kept.tiles));
    }
    if kept.width <= px(0.) {
        return None;
    }
    let run = TextRun {
        len: kept.text.len(),
        font: kept.font?,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let lines = window
        .text_system()
        .shape_text(kept.text.into(), kept.size, &[run], Some(kept.width), None)
        .ok()?
        .iter()
        .map(|line| line.wrap_boundaries().len() + 1)
        .sum::<usize>();
    let height = kept.line * lines as f32 - kept.trim * 2.;
    Some((Some(Bounds::new(kept.origin, size(kept.width, height))), kept.tiles))
}

/// Where the row a message lands on drew its bubble, in the window: the bubble, its letters and
/// its files.
#[derive(Default)]
pub struct Landing {
    pub bubble: Option<Bounds<Pixels>>,
    pub text: Option<Bounds<Pixels>>,
    pub files: Tiles,
}

impl Landing {
    pub fn set_file(&mut self, index: usize, bounds: Bounds<Pixels>) {
        if self.files.len() <= index {
            self.files.resize(index + 1, None);
        }
        self.files[index] = Some(bounds);
    }
}

/// What the rows know of the message thrown in: the row it lands on, which measures where its
/// bubble lands, and whether they draw its copy in flight, whose files fly on their own.
#[derive(Clone)]
pub struct Mark {
    pub row_id: String,
    pub landing: Rc<RefCell<Landing>>,
    pub copy: bool,
}

/// A canvas that keeps its bounds in the landing.
pub fn measure(landing: &Rc<RefCell<Landing>>, keep: fn(&mut Landing, Bounds<Pixels>)) -> impl IntoElement {
    let landing = landing.clone();
    canvas(move |bounds, _, _| keep(&mut landing.borrow_mut(), bounds), |_, _, _, _| {}).absolute().inset_0()
}

/// A message on its way from the composer to its row.
pub struct ThrownMessage {
    /// The row it lands on: the message on its way, then the one the server has.
    pub row_id: String,
    model: Arc<RowModel>,
    /// Where its letters were written, and its tiles, in the window.
    source: Option<Bounds<Pixels>>,
    tiles: Tiles,
    landing: Rc<RefCell<Landing>>,
    /// The bubble where it starts, once its row has said where its letters are in it.
    start: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// How far it is on its way, which only starts on the first frame after the send.
    elapsed: f32,
    last_tick: Option<Instant>,
    /// Where the viewport's top set out from, while it goes to the end with the message.
    pub scrolled_from: Option<f32>,
}

impl ThrownMessage {
    /// Throws `model` from where its text is still written in the composer.
    pub fn new(model: Arc<RowModel>, window: &Window) -> Option<Self> {
        let text = match &model.row.kind {
            RowKind::User { text, .. } | RowKind::Queued { text, .. } => text,
            _ => return None,
        };
        let (source, tiles) = written(text, window)?;
        Some(Self {
            row_id: model.row.id.clone(),
            model,
            source,
            tiles,
            landing: Rc::default(),
            start: Rc::default(),
            elapsed: 0.,
            last_tick: None,
            scrolled_from: None,
        })
    }

    pub fn mark(&self, copy: bool) -> Mark {
        Mark { row_id: self.row_id.clone(), landing: self.landing.clone(), copy }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_tick.replace(now) {
            self.elapsed += (now - last).as_secs_f32().min(LONGEST_FRAME);
        }
    }

    pub fn done(&self) -> bool {
        self.elapsed >= DURATION
    }

    /// How far along the viewport is on its way to the end, which it travels with the message.
    pub fn scrolled(&self) -> f32 {
        TRAVEL.progress(self.elapsed - LIFT)
    }

    /// The copy in flight, drawn where it is by now on its way to its row.
    pub fn flight(&self, ctx: &RowContext) -> Flight {
        let mut ctx = ctx.clone();
        ctx.throw = Some(self.mark(true));
        Flight {
            model: self.model.clone(),
            ctx,
            time: self.elapsed,
            source: self.source,
            tiles: self.tiles.clone(),
            landing: self.landing.clone(),
            start: self.start.clone(),
        }
    }
}

/// The copy of a thrown message, placed in its prepaint, once its row has drawn this frame.
pub struct Flight {
    model: Arc<RowModel>,
    ctx: RowContext,
    time: f32,
    source: Option<Bounds<Pixels>>,
    tiles: Tiles,
    landing: Rc<RefCell<Landing>>,
    start: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Flight {
    /// Where the bubble starts: around what was written as it is around its text, so that the
    /// text lies where it was written, or around the tiles as it is around its files.
    fn start(&self, landing: &Landing, target: Bounds<Pixels>, reshapes: bool) -> Option<Bounds<Pixels>> {
        let within = |bounds: Bounds<Pixels>| Bounds::new(bounds.origin - target.origin, bounds.size);
        let (in_bubble, in_composer) = match self.source {
            Some(source) => (within(landing.text?), source),
            None => {
                let known = landing.files.iter().zip(&self.tiles).filter_map(|(file, tile)| Some(((*file)?, (*tile)?)));
                let (files, tiles): (Vec<_>, Vec<_>) = known.map(|(file, tile)| (within(file), tile)).unzip();
                (files.into_iter().reduce(|a, b| a.union(&b))?, tiles.into_iter().reduce(|a, b| a.union(&b))?)
            }
        };
        let size = match reshapes {
            true => size(
                in_composer.size.width + in_bubble.left() + target.size.width - in_bubble.right(),
                in_composer.size.height + in_bubble.top() + target.size.height - in_bubble.bottom(),
            ),
            false => target.size,
        };
        Some(Bounds::new(in_composer.origin - in_bubble.origin, size))
    }

    /// The copy, and where it is by now in the window before it is scaled.
    fn place(&self, window: &mut Window, cx: &mut App) -> Option<(AnyElement, Bounds<Pixels>)> {
        let landing = self.landing.borrow();
        let target = landing.bubble?;
        let (bubble, reshapes) = rows::thrown_bubble(&self.model, &self.ctx, window, cx)?;
        let start = match self.start.get() {
            Some(start) => start,
            None => self.start(&landing, target, reshapes)?,
        };
        self.start.set(Some(start));
        // A taller composer throws a message that shrinks less, as in Messages.
        let smallest = (0.7 + (f32::from(start.size.height) - 40.25) * 0.000827).clamp(0.7, 0.9);

        let time = self.time;
        let reshaped = RESHAPE.value(time / RESHAPE_TIME);
        let between = |from: Pixels, to: Pixels, progress: f32| from + (to - from) * progress;
        let shape = size(
            between(start.size.width, target.size.width, reshaped),
            between(start.size.height, target.size.height, reshaped),
        );
        let right = between(start.right(), target.right(), TRAVEL.progress(time));
        let middle = between(start.center().y, target.center().y, TRAVEL.progress(time - LIFT));
        let scale = (1. + (smallest - 1.) * SHRINK.progress(time))
            * (1. + (1. / smallest - 1.) * REGROW.progress(time - REGROW_AFTER));
        let placed = Bounds::new(point(right - shape.width, middle - shape.height / 2.), shape);

        let fill = crate::theme::colors(cx).background_secondary.opacity(reshaped);
        let files = rows::thrown_files(&self.model, &self.ctx, &landing.files, window, cx).into_iter().map(
            |(index, file, pictured, to)| {
                let to = Bounds::new(to.origin - target.origin, to.size);
                let tile = self.tiles.get(index).copied().flatten();
                let from = tile.map_or(to, |tile| Bounds::new(tile.origin - start.origin, tile.size));
                let center = point(
                    between(from.center().x, to.center().x, reshaped),
                    between(from.center().y, to.center().y, reshaped),
                );
                let first =
                    (from.size.width / to.size.width.max(px(1.))).min(from.size.height / to.size.height.max(px(1.)));
                let grown = first + (1. - first) * reshaped;
                div()
                    .absolute()
                    .left(center.x - to.size.width / 2.)
                    .top(center.y - to.size.height / 2.)
                    .w(to.size.width)
                    .h(to.size.height)
                    .opacity(if pictured { 1. } else { reshaped })
                    .child(Scaled::new(file, stepped(grown), point(0.5, 0.5)))
            },
        );
        let copy = div()
            .relative()
            .size_full()
            .when(reshapes, |copy| copy.rounded(px(rows::BUBBLE_RADIUS)).bg(fill))
            .child(div().absolute().top_0().left_0().child(bubble))
            .children(files);
        Some((Scaled::new(copy, stepped(scale), point(1., 0.5)).into_any_element(), placed))
    }
}

fn stepped(scale: f32) -> f32 {
    (scale * SCALE_STEPS).round() / SCALE_STEPS
}

impl IntoElement for Flight {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Flight {
    type RequestLayoutState = ();
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style { position: Position::Absolute, ..Style::default() };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let (mut copy, placed) = self.place(window, cx)?;
        copy.layout_as_root(placed.size.map(AvailableSpace::Definite), window, cx);
        copy.prepaint_at(placed.origin, window, cx);
        Some(copy)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        copy: &mut Option<AnyElement>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(copy) = copy {
            copy.paint(window, cx);
        }
    }
}

/// An element drawn `scale` times its size around a point of it, given as a fraction of its
/// width and height. It takes no part in hit testing.
pub struct Scaled {
    child: AnyElement,
    scale: f32,
    around: Point<f32>,
}

impl Scaled {
    pub fn new(child: impl IntoElement, scale: f32, around: Point<f32>) -> Self {
        Self { child: child.into_any_element(), scale: scale.max(0.01), around }
    }
}

impl IntoElement for Scaled {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Scaled {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
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
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let around = point(
            bounds.origin.x + bounds.size.width * self.around.x,
            bounds.origin.y + bounds.size.height * self.around.y,
        );
        window.prepaint_scaled(around, self.scale, |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.paint_scaled(self.scale, |window| self.child.paint(window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{REGROW, RESHAPE, SHRINK, TRAVEL};

    #[test]
    fn the_springs_move_as_core_animation_runs_them() {
        assert_eq!(TRAVEL.progress(0.), 0.);
        assert_eq!(TRAVEL.progress(-0.1), 0.);
        assert!((TRAVEL.progress(0.1) - 0.3868).abs() < 0.001);
        assert!((TRAVEL.progress(0.2) - 0.8233).abs() < 0.001);
        // It overshoots a little and settles.
        assert!((TRAVEL.progress(0.5) - 1.0189).abs() < 0.001);
        assert!((TRAVEL.progress(1.) - 1.).abs() < 0.001);
        assert!((SHRINK.progress(0.1) - 0.4028).abs() < 0.001);
        assert!((REGROW.progress(0.1) - 0.4143).abs() < 0.001);
    }

    #[test]
    fn the_curve_eases_in_and_out_between_its_ends() {
        assert_eq!(RESHAPE.value(0.), 0.);
        assert_eq!(RESHAPE.value(1.), 1.);
        assert!((RESHAPE.value(0.54575) - 0.5).abs() < 0.001);
        assert!(RESHAPE.value(0.2) < 0.2 && RESHAPE.value(0.8) > 0.8);
    }
}
