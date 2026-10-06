//! Text with styled stretches that the user can select and copy, and links that open with a
//! click. It takes part in the window's one selection, like GPUI Kit's `SelectableText`.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::base::{TextSelection, TextSelectionHandle, TextSelectionRegistration, TextSelectionRun};
use gpui_kit::prelude::*;
use gpui_kit::*;

type OnLink = Rc<dyn Fn(&str, &mut Window, &mut App)>;

pub struct RichText {
    id: ElementId,
    text: SharedString,
    styled: StyledText,
    links: Rc<Vec<(Range<usize>, String)>>,
    on_link: Option<OnLink>,
    /// Stretches drawn on a rounded tint as tall as their glyphs, as the Mac draws inline code.
    tinted: Vec<Range<usize>>,
    tint: Hsla,
}

impl RichText {
    /// Text in runs of fonts and colours that cover all of it.
    pub fn with_runs(id: impl Into<ElementId>, text: impl Into<SharedString>, runs: Vec<TextRun>) -> Self {
        let text = text.into();
        Self {
            id: id.into(),
            styled: StyledText::new(text.clone()).with_runs(runs),
            text,
            links: Rc::new(Vec::new()),
            on_link: None,
            tinted: Vec::new(),
            tint: Hsla::transparent_black(),
        }
    }

    pub fn tinted(mut self, ranges: Vec<Range<usize>>, tint: Hsla) -> Self {
        self.tinted = ranges;
        self.tint = tint;
        self
    }

    /// The tint behind a stretch, a rounded box on each line it reaches.
    fn paint_tint(layout: &TextLayout, range: Range<usize>, height: Pixels, color: Hsla, window: &mut Window) {
        let (Some(mut start), Some(end)) =
            (layout.position_for_index(range.start), layout.position_for_index(range.end))
        else {
            return;
        };
        let bounds = layout.bounds();
        let line_height = layout.line_height();
        if let Some(after) = layout.position_for_index(range.start + 1)
            && after.y > start.y
        {
            start = point(bounds.left(), after.y);
        }
        let inset = (line_height - height) / 2.;
        let pad = px(2.);
        let mut boxes = Vec::new();
        let mut top = start.y;
        while top <= end.y {
            let left = if top == start.y { start.x } else { bounds.left() };
            let right = if top == end.y { end.x } else { bounds.right() };
            if right > left {
                boxes.push(Bounds::from_corners(
                    point(left - pad, top + inset),
                    point(right + pad, top + inset + height),
                ));
            }
            top += line_height;
        }
        for tint in boxes {
            window.paint_quad(fill(tint, color).corner_radii(px(4.)));
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
    type RequestLayoutState = TextSelectionHandle;
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
        let handle = window.with_element_state(
            global_id.expect("RichText has an id"),
            |retained: Option<TextSelectionHandle>, _| {
                let handle = retained.unwrap_or_else(|| TextSelectionHandle::new(self.text.clone(), cx));
                (handle.clone(), handle)
            },
        );
        handle.set_fallback_copy_text(self.text.to_string(), cx);
        let (layout_id, ()) = self.styled.request_layout(global_id, inspector_id, window, cx);
        (layout_id, handle)
    }

    fn prepaint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled.prepaint(global_id, inspector_id, bounds, &mut (), window, cx);
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let registration = TextSelectionRegistration::new(hitbox.clone(), bounds)
            .with_text_bounds(vec![bounds])
            .with_rendered_element(handle, window, cx);
        handle.register(registration, window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        handle: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let layout = self.styled.layout().clone();
        let selected_before = TextSelection::selected_text(window, cx);
        let projection = handle.update_runs(&[TextSelectionRun::new(self.text.clone(), layout.clone(), bounds)], cx);
        if selected_before != TextSelection::selected_text(window, cx) {
            window.refresh();
        }
        let glyphs = window.text_style().font_size.to_pixels(window.rem_size()) * 1.22;
        for range in self.tinted.clone() {
            Self::paint_tint(&layout, range, glyphs, self.tint, window);
        }
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
        let pressed: Rc<std::cell::Cell<Option<Point<Pixels>>>> = Rc::default();
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
            let Some(start) = pressed.take() else { return };
            if !phase.bubble() || event.button != MouseButton::Left {
                return;
            }
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

// Markdown a panel shows, a pull request's description or an issue's, drawn as the transcript
// draws a reply but at the panel's size.

use crate::theme::{self, colors, is_dark};
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
                c.link
            } else if code || bold {
                c.text
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
    let tint = if is_dark(cx) { hsla(0., 0., 1., 0.1) } else { hsla(0., 0., 0., 0.06) };
    let code: Vec<Range<usize>> =
        para.runs.iter().filter(|(_, bits)| prose::is_code(*bits)).map(|(range, _)| range.clone()).collect();
    RichText::with_runs(id, para.text.clone(), prose_runs(para, color, weight, cx))
        .links(para.links.clone(), |url, _, cx| cx.open_url(url))
        .tinted(code, tint)
        .into_any_element()
}

/// Code with its colours in a box on the next surface, its long lines wrapped.
pub fn code_block(id: &str, code: &SharedString, spans: &[(Range<usize>, u32)], cx: &App) -> Div {
    let c = colors(cx);
    let base = panel_font(theme::MONO_FONT, FontWeight::NORMAL, false);
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, colour) in spans {
        if range.start < at || range.end > code.len() {
            continue;
        }
        if range.start > at {
            runs.push(plain_run(range.start - at, base.clone(), c.text));
        }
        let color = c.syntax.get(*colour as usize).copied().unwrap_or(c.text);
        runs.push(plain_run(range.end - range.start, base.clone(), color));
        at = range.end;
    }
    if at < code.len() {
        runs.push(plain_run(code.len() - at, base, c.text));
    }
    div()
        .w_full()
        .p(px(10.))
        .bg(c.background_secondary)
        .rounded(px(theme::Radius::CARD))
        .font_family(theme::MONO_FONT)
        .text_size(px(12.))
        .line_height(px(17.))
        .text_color(c.text)
        .when(!code.is_empty(), |block| {
            block.child(RichText::with_runs(SharedString::from(format!("{id}-code")), code.clone(), runs))
        })
}

/// Prose at `size`: its paragraphs, lists, quotes, code, rules and tables.
pub fn prose_blocks(id: &str, prepared: &PreparedProse, size: f32, cx: &App) -> Div {
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
                        (heading, (heading * 1.25).round(), FontWeight::SEMIBOLD, c.text, 6.)
                    }
                    ParaStyle::Body => (size, line, FontWeight::NORMAL, c.prose, 8.),
                    ParaStyle::ListItem { .. } => (size, line, FontWeight::NORMAL, c.prose, 3.),
                    ParaStyle::Quote(_) => (size, line, FontWeight::NORMAL, c.secondary, 6.),
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
                        .children((0..depth).map(|_| div().w(px(2.)).h_full().bg(c.border_secondary)))
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
                .child(code_block(&block_id, &code.code, code.spans.as_deref().unwrap_or(&[]), cx))
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
                                let color = if cell.header { c.text } else { c.prose };
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

/// Markdown the core set as `Text` blocks, the panel's descriptions and comments, at the
/// panel's size.
pub fn markdown(id: &str, blocks: &[motile_core::pull_request::Text], cx: &App) -> Div {
    use motile_core::pull_request::Text;
    let mut column = div().w_full().flex().flex_col().gap(px(10.));
    for (index, block) in blocks.iter().enumerate() {
        let block_id = format!("{id}-{index}");
        let element = match block {
            Text::Prose { prose: text } => prose_blocks(&block_id, &prose::prepare(text), 13., cx),
            Text::Code { code, .. } => code_block(&block_id, &SharedString::from(code.clone()), &[], cx),
        };
        column = column.child(element);
    }
    column
}
