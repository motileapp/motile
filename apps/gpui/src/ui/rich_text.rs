//! Text with styled stretches that the user can select and copy, and links that open with a
//! click. It takes part in the window's one selection, like GPUI Kit's `SelectableText`. Text of a
//! transcript's row is selected only with the rest of its row, as the Mac's text view of a row
//! is: once a press falls in a row, the text of the others stands apart. A right click on a
//! selection offers to copy it.

use std::ops::Range;
use std::rc::Rc;
use std::sync::OnceLock;

use gpui_kit::base::{
    TextSelection, TextSelectionHandle, TextSelectionRegistration, TextSelectionRun, TextSelectionScopeId,
};
use gpui_kit::prelude::*;
use gpui_kit::*;

type OnLink = Rc<dyn Fn(&str, &mut Window, &mut App)>;
type LinkPress = Rc<std::cell::Cell<Option<Point<Pixels>>>>;

pub struct RichText {
    id: ElementId,
    text: SharedString,
    styled: StyledText,
    links: Rc<Vec<(Range<usize>, String)>>,
    on_link: Option<OnLink>,
    /// Stretches drawn on a rounded tint as tall as their glyphs, as the Mac draws inline code.
    tinted: Vec<Range<usize>>,
    tint: Hsla,
    /// The height of the tint's glyphs, when they are smaller than the text's.
    tint_glyphs: Option<Pixels>,
    /// The names of skills, drawn on a chip's box with room left for it around them.
    skills: Vec<Range<usize>>,
    /// The transcript row it is in.
    row: Option<u64>,
    /// Where it comes in the window's text, which a copied selection keeps to.
    order: u64,
}

/// The transcript row the last press fell in, if it fell in one.
#[derive(Default)]
struct PressedRow(Option<u64>);

impl Global for PressedRow {}

/// Says where a press fell: in a transcript's row, or elsewhere. The window's root says it of
/// every press before the row does.
pub fn press_row(row: Option<u64>, cx: &mut App) {
    cx.default_global::<PressedRow>().0 = row;
}

/// Ends the selection a press started when the button comes up, wherever it does. A control that
/// takes the click keeps the release from the selection, which would follow the pointer on without it.
pub fn watch_releases(window: &mut Window) {
    window.on_mouse_event(|event: &MouseUpEvent, phase, window, cx| {
        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
            TextSelection::end(window, cx);
        }
    });
}

/// What text takes the keys with once a drag starts in it, inside the window's root, whose copy
/// shortcut then copies the selection.
struct SelectionFocus(FocusHandle);

impl Global for SelectionFocus {}

pub fn selection_focus(cx: &mut App) -> FocusHandle {
    if let Some(focus) = cx.try_global::<SelectionFocus>() {
        return focus.0.clone();
    }
    let focus = cx.focus_handle();
    cx.set_global(SelectionFocus(focus.clone()));
    focus
}

thread_local! {
    static PAINTED: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The next place in the window's text: text is laid out in the order it reads.
fn next_order() -> u64 {
    PAINTED.with(|painted| {
        painted.set(painted.get() + 1);
        painted.get()
    })
}

/// The scope text takes part in a selection with: the window's, unless it is in a transcript row
/// other than the one pressed last, whose text keeps apart in a scope never active.
fn scope_of(row: Option<u64>, cx: &App) -> TextSelectionScopeId {
    static APART: OnceLock<TextSelectionScopeId> = OnceLock::new();
    let pressed = cx.try_global::<PressedRow>().and_then(|pressed| pressed.0);
    match (row, pressed) {
        (Some(row), Some(pressed)) if row != pressed => *APART.get_or_init(TextSelectionScopeId::new),
        _ => TextSelectionScopeId::default(),
    }
}

/// What a `RichText` keeps between frames: its part in the selection, the text it said it has,
/// what a copy of it takes and where a press on a link fell.
#[derive(Clone)]
pub struct Kept {
    handle: TextSelectionHandle,
    text: SharedString,
    copied: Rc<std::cell::RefCell<Copied>>,
    _redraws: Rc<Subscription>,
    link_press: LinkPress,
}

/// The text a copy takes from, with its links, and the part of it selected when it last painted.
#[derive(Default)]
struct Copied {
    text: SharedString,
    links: Rc<Vec<(Range<usize>, String)>>,
    selected: Option<Range<usize>>,
}

impl Copied {
    fn markdown(&self) -> String {
        let range = self.selected.clone().unwrap_or(0..self.text.len());
        with_links(&self.text, range, &self.links)
    }
}

/// The text of `range`, with the links in it written as Markdown's.
fn with_links(text: &str, range: Range<usize>, links: &[(Range<usize>, String)]) -> String {
    let mut copied = String::new();
    let mut at = range.start;
    let mut links: Vec<_> = links.iter().collect();
    links.sort_by_key(|(link, _)| link.start);
    for (link, url) in links {
        let (start, end) = (link.start.max(at), link.end.min(range.end));
        if start >= end {
            continue;
        }
        copied.push_str(&text[at..start]);
        let words = &text[start..end];
        let bare = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
        if words == url || words == bare {
            copied.push_str(words);
        } else {
            copied.push_str(&format!("[{words}]({url})"));
        }
        at = end;
    }
    copied.push_str(&text[at..range.end]);
    copied
}

impl RichText {
    /// Text in the window's text style.
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        Self::styled(id, text.clone(), StyledText::new(text))
    }

    /// Text in runs of fonts and colours that cover all of it.
    pub fn with_runs(id: impl Into<ElementId>, text: impl Into<SharedString>, runs: Vec<TextRun>) -> Self {
        let text = text.into();
        Self::styled(id, text.clone(), StyledText::new(text).with_runs(runs))
    }

    fn styled(id: impl Into<ElementId>, text: SharedString, styled: StyledText) -> Self {
        Self {
            id: id.into(),
            styled,
            text,
            links: Rc::new(Vec::new()),
            on_link: None,
            tinted: Vec::new(),
            tint: Hsla::transparent_black(),
            tint_glyphs: None,
            skills: Vec::new(),
            row: None,
            order: 0,
        }
    }

    /// The transcript row it is in, which a selection keeps to.
    pub fn row(mut self, row: u64) -> Self {
        self.row = Some(row);
        self
    }

    pub fn tinted(mut self, ranges: Vec<Range<usize>>, tint: Hsla) -> Self {
        self.tinted = ranges;
        self.tint = tint;
        self
    }

    /// Sizes the tint for glyphs `size` big rather than the text's.
    pub fn tint_size(mut self, size: Pixels) -> Self {
        self.tint_glyphs = Some(size * 1.22);
        self
    }

    /// Where its glyphs are, once it has been laid out this frame.
    pub fn text_layout(&self) -> TextLayout {
        self.styled.layout().clone()
    }

    pub fn skills(mut self, names: Vec<Range<usize>>) -> Self {
        self.skills = names;
        self
    }

    /// The tint behind a stretch, a rounded box on each line it reaches, as long as its glyphs.
    fn paint_tint(
        text: &str,
        layout: &TextLayout,
        range: Range<usize>,
        height: Pixels,
        color: Hsla,
        window: &mut Window,
    ) {
        let Some(line) = layout.line_layout_for_index(range.start) else { return };
        let line_start = text[..range.start.min(text.len())].rfind('\n').map_or(0, |at| at + 1);
        let breaks: Vec<usize> = line
            .wrap_boundaries
            .iter()
            .filter_map(|wrap| line.unwrapped_layout.runs.get(wrap.run_ix)?.glyphs.get(wrap.glyph_ix))
            .map(|glyph| line_start + glyph.index)
            .filter(|at| *at > range.start && *at < range.end)
            .collect();
        let line_height = layout.line_height();
        let (inset, pad) = ((line_height - height) / 2., px(2.));
        let left = layout.bounds().left();
        let starts = std::iter::once(range.start).chain(breaks.iter().copied());
        let ends = breaks.iter().copied().chain(std::iter::once(range.end));
        for (index, (from, to)) in starts.zip(ends).enumerate() {
            // An index where a line wraps is placed at the end of the line before it.
            let Some(end) = layout.position_for_index(to) else { continue };
            let from_x = match index {
                0 => match layout.position_for_index(from) {
                    Some(start) if start.y == end.y => start.x,
                    _ => left,
                },
                _ => left,
            };
            if end.x > from_x {
                let tint = Bounds::from_corners(
                    point(from_x - pad, end.y + inset),
                    point(end.x + pad, end.y + inset + height),
                );
                window.paint_quad(fill(tint, color).corner_radii(px(4.)));
            }
        }
    }

    pub fn links(
        mut self,
        links: Vec<(Range<usize>, String)>,
        on_link: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.links = Rc::new(links);
        self.on_link = Some(Rc::new(on_link));
        self
    }

    /// Once a press said which row it fell in, the text of a row says again, before the press
    /// can start a selection, whether it takes part in it. A right click on text with the
    /// selection in it offers to copy the selection.
    fn listen_for_presses(
        &self,
        handle: &TextSelectionHandle,
        hitbox: &Hitbox,
        bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        if let Some(row) = self.row {
            let (handle, registered, order) = (handle.clone(), hitbox.clone(), self.order);
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if !phase.capture() || event.button != MouseButton::Left {
                    return;
                }
                let registration = TextSelectionRegistration::new(registered.clone(), bounds)
                    .with_scope(scope_of(Some(row), cx))
                    .with_document_order(order)
                    .with_text_bounds(vec![bounds]);
                handle.register(registration, window, cx);
            });
        }
        let (handle, hitbox) = (handle.clone(), hitbox.clone());
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if !phase.bubble() || event.button != MouseButton::Right || !hitbox.is_hovered(window) {
                return;
            }
            if handle.snapshot(cx).is_none() {
                return;
            }
            let selected = TextSelection::selected_text(window, cx);
            if selected.trim().is_empty() {
                return;
            }
            cx.stop_propagation();
            text_menu(selected).show(event.position, window, cx);
        });
    }

    fn paint_selection(layout: &TextLayout, range: Range<usize>, color: Hsla, window: &mut Window) {
        let (Some(start), Some(end)) = (layout.position_for_index(range.start), layout.position_for_index(range.end))
        else {
            return;
        };
        let bounds = layout.bounds();
        let line_height = layout.line_height();
        let mut quads = Vec::new();
        if start.y == end.y {
            quads.push(Bounds::from_corners(start, point(end.x, end.y + line_height)));
        } else {
            quads.push(Bounds::from_corners(start, point(bounds.right(), start.y + line_height)));
            if end.y > start.y + line_height {
                quads.push(Bounds::from_corners(
                    point(bounds.left(), start.y + line_height),
                    point(bounds.right(), end.y),
                ));
            }
            quads.push(Bounds::from_corners(point(bounds.left(), end.y), point(end.x, end.y + line_height)));
        }
        for quad in quads {
            window.paint_quad(fill(quad, color));
        }
    }
}

impl IntoElement for RichText {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for RichText {
    type RequestLayoutState = Kept;
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let (kept, changed) =
            window.with_element_state(global_id.expect("RichText has an id"), |kept: Option<Kept>, window| {
                let kept = kept.unwrap_or_else(|| {
                    let handle = TextSelectionHandle::new(self.text.clone(), cx);
                    // Selecting takes the keys from a field, as a text view of the Mac's becomes the
                    // first responder, so that ⌘C copies the selection.
                    handle.focus_with(|window, cx| window.focus(&selection_focus(cx), cx), cx);
                    let copied = Rc::new(std::cell::RefCell::new(Copied::default()));
                    let source = copied.clone();
                    handle.copy_with(move |_| source.borrow().markdown(), cx);
                    let redraws = Rc::new(handle.refresh_window_on_change(window, cx));
                    Kept { handle, text: self.text.clone(), copied, _redraws: redraws, link_press: Rc::default() }
                });
                let changed = kept.text != self.text;
                let kept = Kept { text: self.text.clone(), ..kept };
                ((kept.clone(), changed), kept)
            });
        if changed {
            kept.handle.set_fallback_copy_text(self.text.to_string(), cx);
        }
        {
            let mut copied = kept.copied.borrow_mut();
            copied.text = self.text.clone();
            copied.links = self.links.clone();
        }
        let (layout_id, ()) = self.styled.request_layout(global_id, inspector_id, window, cx);
        (layout_id, kept)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        kept: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled.prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.order = next_order();
        let registration = TextSelectionRegistration::new(hitbox.clone(), bounds)
            .with_scope(scope_of(self.row, cx))
            .with_document_order(self.order)
            .with_text_bounds(vec![bounds])
            .with_rendered_element(&kept.handle, window, cx);
        kept.handle.register(registration, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        kept: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.styled.layout().clone();
        let projection =
            kept.handle.update_runs(&[TextSelectionRun::new(self.text.clone(), layout.clone(), bounds)], cx);
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        self.listen_for_presses(&kept.handle, hitbox, bounds, window);
        let glyphs =
            self.tint_glyphs.unwrap_or_else(|| window.text_style().font_size.to_pixels(window.rem_size()) * 1.22);
        for range in self.tinted.clone() {
            Self::paint_tint(&self.text, &layout, range, glyphs, self.tint, window);
        }
        for name in &self.skills {
            let (Some(start), Some(end)) = (layout.position_for_index(name.start), layout.position_for_index(name.end))
            else {
                continue;
            };
            if start.y == end.y {
                crate::ui::skill_chip::paint_box(start, end, layout.line_height(), window, cx);
            }
        }
        kept.copied.borrow_mut().selected = projection.ranges().iter().flatten().next().cloned();
        let color = gpui_kit::base::Theme::global(cx).tokens.colors.selection;
        for range in projection.ranges().iter().flatten().cloned() {
            Self::paint_selection(&layout, range, color, window);
        }
        self.styled.paint(global_id, inspector_id, bounds, &mut (), &mut (), window, cx);

        if self.links.is_empty() {
            return;
        }
        let link_at = {
            let links = self.links.clone();
            let layout = layout.clone();
            move |position: Point<Pixels>| -> Option<String> {
                let index = layout.index_for_position(position).ok()?;
                links.iter().find(|(range, _)| range.contains(&index)).map(|(_, url)| url.clone())
            }
        };
        if hitbox.is_hovered(window) && link_at(window.mouse_position()).is_some() {
            window.set_cursor_style(CursorStyle::PointingHand, hitbox);
        }
        let Some(on_link) = self.on_link.clone() else { return };
        let hitbox = hitbox.clone();
        let pressed = kept.link_press.clone();
        let down = pressed.clone();
        let down_link = link_at.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
            if phase.bubble()
                && event.button == MouseButton::Left
                && hitbox.is_hovered(window)
                && down_link(event.position).is_some()
            {
                down.set(Some(event.position));
            }
        });
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if !phase.bubble() || event.button != MouseButton::Left {
                return;
            }
            let Some(start) = pressed.take() else { return };
            // A drag selects; only a click opens the link.
            let moved = (event.position.x - start.x).abs() + (event.position.y - start.y).abs();
            if moved > px(4.) {
                return;
            }
            if let Some(url) = link_at(event.position) {
                on_link(&url, window, cx);
            }
        });
    }
}

/// What a right click on selected text offers, in the order the Mac's text views offer it.
fn text_menu(selected: String) -> crate::ui::menu::Menu {
    let query = search_url(&selected);
    crate::ui::menu::Menu::new()
        .symbol_item("Search With Google", "search", move |_, cx| cx.open_url(&query))
        .separator()
        .symbol_item("Copy", "copy", move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(selected.clone())))
}

/// Google's results for the words.
fn search_url(words: &str) -> String {
    let query: String = words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect();
    format!("https://www.google.com/search?q={query}")
}

// Markdown a panel shows, a pull request's description or an issue's, drawn as the transcript
// draws a reply but at the panel's size.

use crate::theme::{self, colors};
use crate::transcript::prose::{self, ParaStyle, PreparedProse, ProseBlock, TextPara};

fn panel_font(family: &str, weight: FontWeight, italic: bool) -> Font {
    Font {
        family: SharedString::from(family.to_string()),
        features: FontFeatures::default(),
        fallbacks: None,
        weight,
        style: if italic { FontStyle::Italic } else { FontStyle::Normal },
    }
}

fn plain_run(len: usize, font: Font, color: Hsla) -> TextRun {
    TextRun { len, font, color, background_color: None, underline: None, strikethrough: None }
}

fn prose_runs(para: &TextPara, color: Hsla, weight: FontWeight, cx: &App) -> Vec<TextRun> {
    let c = colors(cx);
    let text = para.text.as_ref();
    let mut cuts: Vec<usize> = vec![0, text.len()];
    cuts.extend(para.runs.iter().flat_map(|(range, _)| [range.start, range.end]));
    cuts.extend(para.links.iter().flat_map(|(range, _)| [range.start, range.end]));
    cuts.retain(|cut| *cut <= text.len() && text.is_char_boundary(*cut));
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .filter(|pair| pair[1] > pair[0])
        .map(|pair| {
            let (start, end) = (pair[0], pair[1]);
            let covers = |range: &Range<usize>| range.start <= start && range.end >= end;
            let bits = para.runs.iter().filter(|(range, _)| covers(range)).fold(0, |bits, (_, run)| bits | run);
            let link = para.links.iter().any(|(range, _)| covers(range));
            let (code, bold) = (prose::is_code(bits), prose::is_bold(bits));
            let font = if code {
                panel_font(theme::MONO_FONT, if bold { FontWeight::SEMIBOLD } else { FontWeight::NORMAL }, false)
            } else {
                panel_font(theme::UI_FONT, if bold { FontWeight::SEMIBOLD } else { weight }, prose::is_italic(bits))
            };
            let run_color = if link {
                c.primary
            } else if code || bold {
                c.foreground
            } else {
                color
            };
            let mut run = plain_run(end - start, font, run_color);
            run.strikethrough =
                prose::is_strike(bits).then(|| StrikethroughStyle { thickness: px(1.), color: Some(run_color) });
            run
        })
        .collect()
}

fn prose_paragraph(id: ElementId, para: &TextPara, color: Hsla, weight: FontWeight, cx: &App) -> AnyElement {
    if para.text.is_empty() {
        return div().into_any_element();
    }
    let tint = colors(cx).background_accent;
    let code: Vec<Range<usize>> =
        para.runs.iter().filter(|(_, bits)| prose::is_code(*bits)).map(|(range, _)| range.clone()).collect();
    RichText::with_runs(id, para.text.clone(), prose_runs(para, color, weight, cx))
        .links(para.links.clone(), |url, _, cx| cx.open_url(url))
        .tinted(code, tint)
        .into_any_element()
}

/// Code with its colours in a box on the `surface` it lies on, its long lines wrapped.
pub fn code_block(
    id: &str,
    code: &SharedString,
    spans: &[(Range<usize>, u32)],
    surface: theme::Surface,
    cx: &App,
) -> Div {
    let c = colors(cx);
    let base = panel_font(theme::MONO_FONT, FontWeight::NORMAL, false);
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, colour) in spans {
        if range.start < at || range.end > code.len() {
            continue;
        }
        if range.start > at {
            runs.push(plain_run(range.start - at, base.clone(), c.foreground));
        }
        let color = c.syntax.get(*colour as usize).copied().unwrap_or(c.foreground);
        runs.push(plain_run(range.end - range.start, base.clone(), color));
        at = range.end;
    }
    if at < code.len() {
        runs.push(plain_run(code.len() - at, base, c.foreground));
    }
    div()
        .w_full()
        .p(px(10.))
        .bg(surface.layer(theme::Layer::Box, c))
        .rounded(px(theme::Radius::LG))
        .font_family(theme::MONO_FONT)
        .text_size(px(12.))
        .line_height(px(17.))
        .text_color(c.foreground)
        .when(!code.is_empty(), |block| {
            block.child(RichText::with_runs(SharedString::from(format!("{id}-code")), code.clone(), runs))
        })
}

/// Prose at `size`: its paragraphs, lists, quotes, code, rules and tables.
pub fn prose_blocks(id: &str, prepared: &PreparedProse, size: f32, surface: theme::Surface, cx: &App) -> Div {
    let c = colors(cx);
    let line = (size * 1.5).round();
    let count = prepared.blocks.len();
    let mut column = div().w_full().flex().flex_col();
    for (index, block) in prepared.blocks.iter().enumerate() {
        let last = index + 1 == count;
        let block_id = SharedString::from(format!("{id}-{index}"));
        let element = match block {
            ProseBlock::Para(para) => {
                let (text_size, text_line, weight, color, after) = match &para.style {
                    ParaStyle::Heading(level) => {
                        let heading = (theme::heading_size(*level) * size / theme::PROSE_SIZE).round();
                        (heading, (heading * 1.25).round(), FontWeight::SEMIBOLD, c.foreground, 6.)
                    }
                    ParaStyle::Body => (size, line, FontWeight::NORMAL, c.foreground, 8.),
                    ParaStyle::ListItem { .. } => (size, line, FontWeight::NORMAL, c.foreground, 3.),
                    ParaStyle::Quote(_) => (size, line, FontWeight::NORMAL, c.muted_foreground, 6.),
                };
                let body = div()
                    .text_size(px(text_size))
                    .line_height(px(text_line))
                    .text_color(color)
                    .child(prose_paragraph(block_id.into(), para, color, weight, cx));
                let quote_bars = |depth: u8| {
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(1.))
                        .flex()
                        .gap(px(12.))
                        .children((0..depth).map(|_| div().w(px(2.)).h_full().bg(c.border)))
                };
                let element = match &para.style {
                    ParaStyle::ListItem { depth, marker, quote } => {
                        let quote_indent = *quote as f32 * 14.;
                        div()
                            .relative()
                            .pl(px(quote_indent + *depth as f32 * 20.))
                            .when(*quote > 0, |item| item.child(quote_bars(*quote)))
                            .when_some(marker.clone(), |item, marker| {
                                item.child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left(px(quote_indent + (*depth as f32 - 1.) * 20. + 4.))
                                        .text_size(px(text_size))
                                        .line_height(px(text_line))
                                        .text_color(color)
                                        .child(marker),
                                )
                            })
                            .child(body)
                    }
                    ParaStyle::Quote(depth) => {
                        div().relative().pl(px(*depth as f32 * 14.)).child(quote_bars(*depth)).child(body)
                    }
                    _ => body,
                };
                div()
                    .pt(px(para.extra_above))
                    .when(!last, |block| block.pb(px(after)))
                    .child(element)
                    .into_any_element()
            }
            ProseBlock::Code(code) => div()
                .ml(px(code.indent))
                .when(!last, |block| block.pb(px(8.)))
                .child(code_block(&block_id, &code.code, code.spans.as_deref().unwrap_or(&[]), surface, cx))
                .into_any_element(),
            ProseBlock::Rule => div().py(px(6.)).child(div().w_full().h(px(1.)).bg(c.border)).into_any_element(),
            ProseBlock::Table { columns, rows, extra_above } => div()
                .pt(px(*extra_above))
                .when(!last, |block| block.pb(px(8.)))
                .child(div().id(block_id.clone()).overflow_x_scroll().child(div().flex().flex_col().children(
                    rows.iter().enumerate().map(|(row_index, cells)| {
                        div().flex().border_b_1().border_color(c.border).children((0..*columns).map(|column| {
                            let cell = cells.get(column);
                            div().flex_1().min_w(px(60.)).px(px(8.)).py(px(5.)).when_some(cell, |slot, cell| {
                                let weight = if cell.header { FontWeight::SEMIBOLD } else { FontWeight::NORMAL };
                                let color = c.foreground;
                                slot.text_size(px(size)).line_height(px(line)).text_color(color).child(prose_paragraph(
                                    SharedString::from(format!("{block_id}-{row_index}-{column}")).into(),
                                    &cell.para,
                                    color,
                                    weight,
                                    cx,
                                ))
                            })
                        }))
                    }),
                )))
                .into_any_element(),
        };
        column = column.child(element);
    }
    column
}

#[cfg(test)]
mod tests {
    use core::prelude::v1::test;
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn a_search_takes_the_words_as_they_are_typed() {
        assert_eq!(super::search_url("café  au\nlait?"), "https://www.google.com/search?q=caf%C3%A9+au+lait%3F");
    }

    fn headless_window<V: Render>(
        build: impl FnOnce(&mut Context<V>) -> V,
    ) -> Option<(HeadlessAppContext, WindowHandle<V>)> {
        let Some(text) = crate::transcript::canvas::tests::platform_text() else {
            eprintln!("skipped: GPUI has no text system here");
            return None;
        };
        let mut cx = HeadlessAppContext::new(text);
        let fonts = crate::assets::FONTS.iter().map(|bytes| std::borrow::Cow::Borrowed(*bytes)).collect();
        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.text_system().add_fonts(fonts)
        })
        .expect("the fonts");
        crate::fonts::load();
        let window = cx
            .open_window(size(px(400.), px(100.)), |window, cx| {
                crate::theme::apply(true, window, cx);
                cx.new(build)
            })
            .expect("a window");
        Some((cx, window))
    }

    fn draw<V: 'static>(cx: &mut HeadlessAppContext, window: WindowHandle<V>) {
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .expect("a frame");
    }

    fn send<V: 'static>(cx: &mut HeadlessAppContext, window: WindowHandle<V>, events: Vec<PlatformInput>) {
        cx.update_window(window.into(), |_, window, cx| {
            for event in events {
                window.dispatch_event(event, cx);
            }
        })
        .expect("the events");
    }

    fn click_at(position: Point<Pixels>) -> Vec<PlatformInput> {
        vec![
            PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
            PlatformInput::MouseUp(MouseUpEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            }),
        ]
    }

    /// Text above a button that keeps its click to itself, as the app's buttons do, under the
    /// window's selection as GPUI Kit's root lays it.
    struct Buttoned;

    impl Render for Buttoned {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .font_family(crate::theme::UI_FONT)
                .child(gpui_kit::base::TextSelectionLayer)
                .child(RichText::new("words", "some words to select"))
                .child(div().id("button").mt(px(40.)).size(px(40.)).on_click(|_, _, cx| cx.stop_propagation()))
                .child(canvas(|_, _, _| {}, |_, _, window, _| watch_releases(window)).absolute().size_0())
        }
    }

    #[test]
    fn a_click_on_a_button_selects_nothing_as_the_pointer_moves_on() {
        let Some((mut cx, window)) = headless_window(|_| Buttoned) else { return };
        draw(&mut cx, window);
        send(&mut cx, window, click_at(point(px(20.), px(70.))));
        draw(&mut cx, window);
        let over_words = point(px(80.), px(8.));
        send(
            &mut cx,
            window,
            vec![PlatformInput::MouseMove(MouseMoveEvent { position: over_words, ..Default::default() })],
        );
        draw(&mut cx, window);
        let selected = cx
            .update_window(window.into(), |_, window, cx| TextSelection::has_selection(window, cx))
            .expect("the selection");
        assert!(!selected);
    }

    struct Linked {
        opened: Rc<RefCell<Vec<String>>>,
    }

    impl Render for Linked {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let opened = self.opened.clone();
            div().font_family(crate::theme::UI_FONT).child(gpui_kit::base::TextSelectionLayer).child(
                RichText::new("linked", "open the site")
                    .links(vec![(5..13, "https://motile.app".into())], move |url, _, _| {
                        opened.borrow_mut().push(url.into())
                    }),
            )
        }
    }

    #[test]
    fn a_copy_writes_the_links_it_takes_as_markdown() {
        let links = [(4..8, "https://motile.app/docs".to_string()), (13..23, "https://motile.app".to_string())];
        let text = "see docs and motile.app now";
        assert_eq!(with_links(text, 0..text.len(), &links), "see [docs](https://motile.app/docs) and motile.app now");
        assert_eq!(with_links(text, 6..12, &links), "[cs](https://motile.app/docs) and");
        assert_eq!(with_links(text, 9..12, &links), "and");
    }

    #[test]
    fn a_selection_dragged_over_a_link_copies_it_as_markdown() {
        let opened = Rc::new(RefCell::new(Vec::new()));
        let Some((mut cx, window)) = headless_window(|_| Linked { opened }) else { return };
        draw(&mut cx, window);
        let (from, to) = (point(px(1.), px(8.)), point(px(200.), px(8.)));
        send(
            &mut cx,
            window,
            vec![
                PlatformInput::MouseMove(MouseMoveEvent { position: from, ..Default::default() }),
                PlatformInput::MouseDown(MouseDownEvent {
                    position: from,
                    button: MouseButton::Left,
                    click_count: 1,
                    ..Default::default()
                }),
                PlatformInput::MouseMove(MouseMoveEvent {
                    position: to,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                }),
            ],
        );
        draw(&mut cx, window);
        let copied = cx
            .update_window(window.into(), |_, window, cx| TextSelection::selected_text(window, cx))
            .expect("the selection");
        assert_eq!(copied, "open [the site](https://motile.app)");
    }

    #[test]
    fn a_link_opens_when_the_window_redraws_while_it_is_pressed() {
        let opened = Rc::new(RefCell::new(Vec::new()));
        let Some((mut cx, window)) = headless_window(|_| Linked { opened: opened.clone() }) else { return };
        draw(&mut cx, window);
        let [moved, pressed, released] = click_at(point(px(60.), px(8.))).try_into().expect("a click");
        send(&mut cx, window, vec![moved, pressed]);
        draw(&mut cx, window);
        send(&mut cx, window, vec![released]);
        assert_eq!(*opened.borrow(), ["https://motile.app"]);
    }
}
