//! How each kind of row is drawn, as the Mac's `RowViews.swift` draws them. A row is as wide as
//! the transcript's column.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::{ChangeEntry, RowKind, Tool};
use motile_protocol::wire::ToolStatus;

use super::model::RowModel;
use super::prose::{self, CodeBox, ParaStyle, ProseBlock, TableCell, TextPara};
use super::view::TranscriptView;
use crate::models::{AttachedFile, duration, elapsed};
use crate::store::Store;
use crate::theme::{self, Colors, ControlSize, Radius, Surface, colors};
use crate::ui::{ActionButton, edges, highlight_in, icons};

/// The room a prose row has around its text, which is what keeps two rows apart.
pub const PROSE_GAP: f32 = 9.;
pub const TOOL_ROW_HEIGHT: f32 = 28.;
/// What a tool row keeps from a paragraph above it, so that the two are a paragraph apart.
pub const AFTER_PROSE: f32 = 8.;
/// The line under a message or a reply with its time and the button that copies it: as tall as
/// the button.
const META_HEIGHT: f32 = 28.;
pub const TURN_END_HEIGHT: f32 = META_HEIGHT + 10.;
const BUBBLE_PADDING: f32 = 14.;
const BUBBLE_RADIUS: f32 = 18.;
const CODE_HEADER: f32 = META_HEIGHT + 8.;
const CODE_BOTTOM: f32 = 12.;
/// The copy icons of the transcript are smaller than their buttons' size says.
const COPY_SYMBOL: f32 = 12.;

/// What drawing a row needs from the transcript around it.
#[derive(Clone)]
pub struct RowContext {
    pub view: WeakEntity<TranscriptView>,
    pub store: Entity<Store>,
    /// The tool calls opened here, to show their input and output.
    pub expanded: Rc<HashSet<String>>,
    /// The row whose copy button has just copied.
    pub copied: Option<String>,
    /// The row whose time and copy button the pointer reveals.
    pub meta_row: Option<String>,
    /// The rows of the transcript, for copying a reply.
    pub rows: Rc<Vec<Arc<RowModel>>>,
}

impl RowContext {
    fn copied(&self, row_id: &str) -> bool {
        self.copied.as_deref() == Some(row_id)
    }

    fn meta_shown(&self, row_id: &str) -> bool {
        self.meta_row.as_deref() == Some(row_id)
    }
}

/// How far GPUI sets a line's glyphs below the top of a line `line` tall, more than the Mac
/// does: the Mac puts a paragraph's line spacing under its lines, GPUI around them. A text is
/// pulled up and in by this much at its top and bottom so its lines fall where the Mac's do.
pub fn trim(size: f32, line: f32) -> f32 {
    ((line - size * 1.2) / 2.).max(0.)
}

/// A box of text `size` big on lines `line` tall, as tall as the Mac's text of that kind.
pub fn text_box(size: f32, line: f32) -> Div {
    let trim = trim(size, line);
    div().text_size(px(size)).line_height(px(line)).mt(px(-trim)).mb(px(-trim))
}

pub fn font(family: &str, weight: FontWeight, italic: bool) -> Font {
    Font {
        family: SharedString::from(family.to_string()),
        features: FontFeatures::default(),
        fallbacks: None,
        weight,
        style: if italic { FontStyle::Italic } else { FontStyle::Normal },
    }
}

/// The app's typeface with digits of one width, so a line doesn't change size with every second.
fn digits_font(weight: FontWeight) -> Font {
    Font { features: FontFeatures(Arc::new(vec![("tnum".into(), 1)])), ..font(theme::UI_FONT, weight, false) }
}

fn run(len: usize, font: Font, color: Hsla) -> TextRun {
    TextRun { len, font, color, background_color: None, underline: None, strikethrough: None }
}

/// "14:03", or with the day when it isn't today.
pub fn stamp(at: f64) -> String {
    use chrono::{Local, TimeZone};
    let Some(time) = Local.timestamp_opt(at as i64, 0).single() else { return String::new() };
    if time.date_naive() == Local::now().date_naive() {
        time.format("%H:%M").to_string()
    } else {
        time.format("%-d %b %Y, %H:%M").to_string()
    }
}

/// The runs of a paragraph: its plain text in `color`, and each styled stretch as the Mac draws
/// it, bold in the text's colour, code in the code font on a tint, links in the link colour.
/// How a paragraph's plain text is set: its font, colour and weight, and whether it is a heading.
struct Plain<'a> {
    family: &'a str,
    color: Hsla,
    weight: FontWeight,
    heading: bool,
}

fn text_runs(
    text: &str,
    runs: &[(Range<usize>, u32)],
    links: &[(Range<usize>, String)],
    plain: Plain,
    cx: &App,
) -> Vec<TextRun> {
    let Plain { family, color, weight, heading } = plain;
    let c = colors(cx);
    let mut cuts: Vec<usize> = vec![0, text.len()];
    for (range, _) in runs {
        cuts.extend([range.start, range.end]);
    }
    for (range, _) in links {
        cuts.extend([range.start, range.end]);
    }
    cuts.retain(|cut| *cut <= text.len() && text.is_char_boundary(*cut));
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|pair| {
            let (start, end) = (pair[0], pair[1]);
            let bits = runs
                .iter()
                .filter(|(range, _)| range.start <= start && range.end >= end)
                .fold(0, |bits, (_, run)| bits | run);
            let link = links.iter().any(|(range, _)| range.start <= start && range.end >= end);
            let code = prose::is_code(bits);
            let bold = prose::is_bold(bits);
            let run_font = if code {
                font(theme::MONO_FONT, if bold || heading { FontWeight::SEMIBOLD } else { FontWeight::NORMAL }, false)
            } else {
                font(family, if bold { FontWeight::SEMIBOLD } else { weight }, prose::is_italic(bits))
            };
            let run_color = if link {
                c.link
            } else if code || bold {
                c.text
            } else {
                color
            };
            TextRun {
                len: end - start,
                font: run_font,
                color: run_color,
                background_color: None,
                underline: None,
                strikethrough: prose::is_strike(bits)
                    .then(|| StrikethroughStyle { thickness: px(1.), color: Some(run_color) }),
            }
        })
        .filter(|run| run.len > 0)
        .collect()
}

fn open_link(url: &str, _: &mut Window, cx: &mut App) {
    cx.open_url(url);
}

/// A paragraph of prose, selectable, its links opening in the browser.
fn paragraph(id: ElementId, para: &TextPara, color: Hsla, weight: FontWeight, heading: bool, cx: &App) -> AnyElement {
    let c = colors(cx);
    if para.text.is_empty() {
        let runs = vec![run(1, font(theme::UI_FONT, weight, false), color)];
        return crate::ui::rich_text::RichText::with_runs(id, " ", runs).into_any_element();
    }
    let runs =
        text_runs(&para.text, &para.runs, &para.links, Plain { family: theme::UI_FONT, color, weight, heading }, cx);
    let code: Vec<Range<usize>> =
        para.runs.iter().filter(|(_, bits)| prose::is_code(*bits)).map(|(range, _)| range.clone()).collect();
    crate::ui::rich_text::RichText::with_runs(id, para.text.clone(), runs)
        .links(para.links.clone(), open_link)
        .tinted(code, c.background_tertiary)
        .into_any_element()
}

/// Plain text, selectable, with its web addresses clickable.
fn plain(
    id: ElementId,
    text: SharedString,
    links: Vec<(Range<usize>, String)>,
    family: &str,
    color: Hsla,
    cx: &App,
) -> AnyElement {
    if text.is_empty() {
        return div().into_any_element();
    }
    let runs = text_runs(&text, &[], &links, Plain { family, color, weight: FontWeight::NORMAL, heading: false }, cx);
    crate::ui::rich_text::RichText::with_runs(id, text, runs).links(links, open_link).into_any_element()
}

/// Code with its colours, selectable.
fn code_text(id: ElementId, code: &SharedString, spans: Option<&Vec<(Range<usize>, u32)>>, c: &Colors) -> AnyElement {
    if code.is_empty() {
        return div().h(px(theme::CODE_LINE_HEIGHT)).into_any_element();
    }
    let base = font(theme::MONO_FONT, FontWeight::NORMAL, false);
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, colour) in spans.into_iter().flatten() {
        if range.start < at || range.end > code.len() {
            continue;
        }
        if range.start > at {
            runs.push(run(range.start - at, base.clone(), c.text));
        }
        let color = c.syntax.get(*colour as usize).copied().unwrap_or(c.text);
        runs.push(run(range.end - range.start, base.clone(), color));
        at = range.end;
    }
    if at < code.len() {
        runs.push(run(code.len() - at, base, c.text));
    }
    crate::ui::rich_text::RichText::with_runs(id, code.clone(), runs).into_any_element()
}

/// The button that copies something, which shows a check mark for a moment once it has.
fn copy_button(
    id: String,
    help: &'static str,
    copied: bool,
    surface: Surface,
    ctx: &RowContext,
    copy: impl Fn(&mut App) + 'static,
) -> ActionButton {
    let view = ctx.view.clone();
    let key = id.clone();
    ActionButton::icon(SharedString::from(format!("copy-{id}")), if copied { "check" } else { "copy" }, help)
        .symbol_size(COPY_SYMBOL)
        .surface(surface)
        .on_click(move |_, _, cx| {
            copy(cx);
            let _ = view.update(cx, |view, cx| view.show_copied(key.clone(), cx));
        })
}

/// The top of a code box: the language, and a button that copies the code.
fn code_header(id: &str, language: &str, code: &SharedString, ctx: &RowContext, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let code = code.to_string();
    div()
        .h(px(CODE_HEADER))
        .pl(px(14.))
        .pr(px(4.))
        .flex()
        .items_center()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .font_family(theme::MONO_FONT)
                .text_size(px(11.5))
                .text_color(c.secondary)
                .truncate()
                .child(if language.is_empty() { "text".to_string() } else { language.to_string() }),
        )
        .child(copy_button(id.to_string(), "Copy code", ctx.copied(id), Surface::Secondary, ctx, move |cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(code.clone()))
        }))
}

fn code_box(id: &str, code: &CodeBox, ctx: &RowContext, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .ml(px(code.indent))
        .mb(px(12.))
        .bg(c.background_secondary)
        .border_1()
        .border_color(c.border)
        .rounded(px(Radius::CARD))
        .overflow_hidden()
        .child(code_header(id, &code.language, &code.code, ctx, cx))
        .child(
            div()
                .id(SharedString::from(format!("{id}-scroll")))
                .overflow_x_scroll()
                .px(px(14.))
                .pb(px(CODE_BOTTOM))
                .text_size(px(theme::CODE_SIZE))
                .line_height(px(theme::CODE_LINE_HEIGHT))
                .whitespace_nowrap()
                .child(code_text(SharedString::from(format!("{id}-code")).into(), &code.code, code.spans.as_ref(), c)),
        )
}

/// A reply's prose: its paragraphs as the Mac sets them.
fn prose_row(model: &RowModel, ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let Some(prepared) = &model.prose else { return div().into_any_element() };
    let above = model.above();
    let row_id = model.row.id.clone();
    let count = prepared.blocks.len();
    let mut column = div().pt(px(3. + above)).pb(px(PROSE_GAP)).flex().flex_col();
    let mut code_index = 0;
    for (index, block) in prepared.blocks.iter().enumerate() {
        let last = index + 1 == count;
        let id: ElementId = SharedString::from(format!("{row_id}-{index}")).into();
        let element = match block {
            ProseBlock::Para(para) => {
                // The Mac keeps a paragraph's line spacing under its last line too, unless it
                // is the last of the row.
                let (size, line, weight, color, before, after) = match &para.style {
                    ParaStyle::Heading(level) => (
                        theme::heading_size(*level),
                        theme::heading_size(*level) * 1.2 + 3.,
                        FontWeight::SEMIBOLD,
                        c.text,
                        if index == 0 { 0. } else { prose::HEADING_GAP },
                        8. + 3.,
                    ),
                    ParaStyle::Body => (
                        theme::PROSE_SIZE,
                        theme::PROSE_LINE_HEIGHT,
                        FontWeight::NORMAL,
                        c.prose,
                        0.,
                        prose::BLOCK_GAP + prose::LINE_SPACING,
                    ),
                    ParaStyle::ListItem { .. } => (
                        theme::PROSE_SIZE,
                        theme::PROSE_LINE_HEIGHT,
                        FontWeight::NORMAL,
                        c.prose,
                        0.,
                        6. + prose::LINE_SPACING,
                    ),
                    ParaStyle::Quote(_) => (
                        theme::PROSE_SIZE,
                        theme::PROSE_LINE_HEIGHT,
                        FontWeight::NORMAL,
                        c.secondary,
                        0.,
                        8. + prose::LINE_SPACING,
                    ),
                };
                let text = paragraph(id, para, color, weight, matches!(para.style, ParaStyle::Heading(_)), cx);
                let body = text_box(size, line).text_color(color).child(text);
                let element = match &para.style {
                    ParaStyle::ListItem { depth, marker, quote } => {
                        let quote_indent = *quote as f32 * 14.;
                        let indent = quote_indent + *depth as f32 * 22.;
                        div()
                            .relative()
                            .pl(px(indent))
                            .when(*quote > 0, |item| item.child(quote_bars(*quote, c)))
                            .when_some(marker.clone(), |item, marker| {
                                item.child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left(px(quote_indent + (*depth as f32 - 1.) * 22. + 4.))
                                        .text_size(px(size))
                                        .line_height(px(line))
                                        .text_color(color)
                                        .child(marker),
                                )
                            })
                            .child(body)
                            .into_any_element()
                    }
                    ParaStyle::Quote(depth) => div()
                        .relative()
                        .pl(px(*depth as f32 * 14.))
                        .child(quote_bars(*depth, c))
                        .child(body)
                        .into_any_element(),
                    _ => body.into_any_element(),
                };
                div()
                    .pt(px(before + para.extra_above))
                    .when(!last, |paragraph| paragraph.pb(px(after)))
                    .child(element)
                    .into_any_element()
            }
            ProseBlock::Code(code) => {
                let key = format!("{row_id}-code-{code_index}");
                code_index += 1;
                code_box(&key, code, ctx, cx).into_any_element()
            }
            ProseBlock::Rule => div()
                .pt(px(4.))
                .pb(px(if last { 0. } else { 10. }))
                .h(px(1. + 4. + 10.))
                .flex()
                .items_center()
                .child(div().w_full().h(px(1.)).bg(c.border))
                .into_any_element(),
            ProseBlock::Table { columns, rows, extra_above } => {
                table(&row_id, index, *columns, rows, *extra_above, last, cx).into_any_element()
            }
        };
        column = column.child(element);
    }
    column.into_any_element()
}

fn quote_bars(depth: u8, c: &Colors) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(1.))
        .flex()
        .gap(px(12.))
        .children((0..depth).map(|_| div().w(px(2.)).h_full().bg(c.border_secondary)))
}

fn table(
    row_id: &str,
    index: usize,
    columns: usize,
    rows: &[Vec<TableCell>],
    extra_above: f32,
    last: bool,
    cx: &App,
) -> impl IntoElement {
    let c = colors(cx);
    div().pt(px(extra_above)).when(!last, |table| table.pb(px(prose::BLOCK_GAP))).child(
        div().id(SharedString::from(format!("{row_id}-table-{index}"))).overflow_x_scroll().child(
            div().flex().flex_col().children(rows.iter().enumerate().map(|(row_index, cells)| {
                div().flex().border_b_1().border_color(c.border).children((0..columns).map(|column| {
                    let cell = cells.get(column);
                    div().flex_1().min_w(px(60.)).px(px(10.)).pt(px(6.)).pb(px(6. + 3.)).text_color(c.prose).when_some(
                        cell,
                        |slot, cell| {
                            let align = cell.align;
                            let weight = if cell.header { FontWeight::SEMIBOLD } else { FontWeight::NORMAL };
                            let color = if cell.header { c.text } else { c.prose };
                            slot.child(
                                text_box(13., 18.6)
                                    .when(align == 1, |text| text.text_center())
                                    .when(align == 2, |text| text.text_right())
                                    .child(paragraph(
                                        SharedString::from(format!("{row_id}-cell-{index}-{row_index}-{column}"))
                                            .into(),
                                        &cell.para,
                                        color,
                                        weight,
                                        cell.header,
                                        cx,
                                    )),
                            )
                        },
                    )
                }))
            })),
        ),
    )
}

/// Under a message or a reply: when it was sent and a button that copies it, at the side the
/// message is on. It is only seen while the pointer is over its message, and the check mark that
/// says it was copied stays to be seen when the pointer has left.
fn message_meta(
    row_id: &str,
    stamp: String,
    trailing: bool,
    help: &'static str,
    ctx: &RowContext,
    cx: &App,
    copy: impl Fn(&mut App) + 'static,
) -> Div {
    let c = colors(cx);
    let copied = ctx.copied(row_id);
    let shown = ctx.meta_shown(row_id) || copied;
    let time = div().px(px(2.)).text_size(px(12.)).text_color(c.tertiary).whitespace_nowrap().child(stamp);
    let button = copy_button(row_id.to_string(), help, copied, Surface::Background, ctx, copy);
    let meta = div().h(px(META_HEIGHT)).w_full().flex().items_center().opacity(if shown { 1. } else { 0. });
    if trailing {
        return meta.justify_end().child(time).child(button);
    }
    meta.child(button).child(time)
}

/// The message and the files of a bubble, as wide as they are up to the widest a bubble may be.
fn bubble_content(
    model: &RowModel,
    text: &str,
    links: Vec<(Range<usize>, String)>,
    attachments: &[AttachedFile],
    color: Hsla,
    ctx: &RowContext,
    cx: &App,
) -> Div {
    let has_files = !attachments.is_empty();
    div()
        .px(px(BUBBLE_PADDING))
        .pt(px(if has_files { BUBBLE_PADDING } else { 10. }))
        .flex()
        .flex_col()
        .gap(px(if has_files && !text.is_empty() { 8. } else { 0. }))
        .when(has_files, |bubble| bubble.child(crate::media::attached_files(&ctx.store, attachments, cx)))
        .when(!text.is_empty(), |bubble| {
            bubble.child(text_box(theme::PROSE_SIZE, 22.).text_color(color).child(plain(
                SharedString::from(format!("{}-text", model.row.id)).into(),
                text.to_string().into(),
                links,
                theme::UI_FONT,
                color,
                cx,
            )))
        })
}

/// A message of the user: a bubble against the column's right edge, as wide as its text, with
/// the images and files it came with above the text, and when it was sent under it.
fn user_row(
    model: &RowModel,
    text: &str,
    links: Vec<(Range<usize>, String)>,
    attachments: &[AttachedFile],
    at: f64,
    ctx: &RowContext,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    let copied_text = text.to_string();
    div()
        .pt(px(14.))
        .pb(px(25.))
        .flex()
        .flex_col()
        .items_end()
        .child(
            bubble_content(model, text, links, attachments, c.text, ctx, cx)
                .max_w(relative(0.8))
                .min_w(px(12. + BUBBLE_PADDING * 2.))
                .pb(px(if text.is_empty() { BUBBLE_PADDING } else { 10. }))
                .bg(c.background_secondary)
                .rounded(px(BUBBLE_RADIUS))
                .when(model.is_pending(), |bubble| bubble.opacity(0.6)),
        )
        .child(
            message_meta(&model.row.id, stamp(at), true, "Copy message", ctx, cx, move |cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copied_text.clone()))
            })
            .mt(px(4.)),
        )
        .into_any_element()
}

/// A message that waits for the agent: what it says, how it waits, and the buttons that steer
/// the turn with it or take it back into the composer. It stands where the user's messages do,
/// dotted around instead of filled.
fn queued_row(
    model: &RowModel,
    text: &str,
    attachments: &[AttachedFile],
    status: &str,
    ctx: &RowContext,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    // The buttons are this far from the bubble's right and bottom, this far from the message,
    // and a click this far above them still presses them.
    let (margin, gap, reach) = (10., 18., 4.);
    let pending = model.is_pending();
    let message = model.row.item.clone();
    let (steer, edit) = (ctx.store.clone(), ctx.store.clone());
    let (steer_id, edit_id) = (message.clone(), message.clone());
    let foot = div()
        .pt(px(gap - reach))
        .pl(px(BUBBLE_PADDING))
        .flex()
        .items_center()
        .child(
            div()
                .flex_1()
                .mt(px(reach))
                .mb(px(margin))
                .h(px(ControlSize::Regular.height()))
                .pr(px(8.))
                .flex()
                .items_center()
                .child(div().w(px(14.)).flex().child(icons::symbol("clock", 11.).text_color(c.secondary)))
                .child(
                    div()
                        .ml(px(4.))
                        .text_size(px(12.))
                        .text_color(c.secondary)
                        .whitespace_nowrap()
                        .child(status.to_string()),
                ),
        )
        .child(
            ActionButton::new(SharedString::from(format!("steer-{message}")), "Steer")
                .help("Send as a steer instead")
                .margin(edges(reach, 3., margin, 3.))
                .on_click(move |_, _, cx| {
                    if pending {
                        return;
                    }
                    steer.update(cx, |store, cx| {
                        store.send_now(&steer_id);
                        cx.notify();
                    })
                }),
        )
        .child(
            ActionButton::new(SharedString::from(format!("edit-{message}")), "Edit")
                .help("Edit in the composer")
                .margin(edges(reach, 3., margin, margin))
                .on_click(move |_, _, cx| {
                    if pending {
                        return;
                    }
                    edit.update(cx, |store, cx| {
                        store.take_back(&edit_id);
                        cx.notify();
                    })
                }),
        );
    div()
        .py(px(14.))
        .flex()
        .justify_end()
        .child(
            div()
                .max_w(relative(0.8))
                .border_1()
                .border_dashed()
                .border_color(c.border_secondary)
                .rounded(px(BUBBLE_RADIUS))
                .flex()
                .flex_col()
                .child(bubble_content(model, text, Vec::new(), attachments, c.prose, ctx, cx))
                .child(foot),
        )
        .into_any_element()
}

/// A code block of a reply.
fn code_row(model: &RowModel, language: &str, code: &str, ctx: &RowContext, cx: &App) -> AnyElement {
    let code = CodeBox {
        language: language.to_string(),
        code: code.to_string().into(),
        spans: model.code_spans.clone(),
        indent: 0.,
    };
    div().pt(px(4.)).pb(px(10.)).child(code_box(&model.row.id, &code, ctx, cx).mb(px(0.))).into_any_element()
}

/// What a row of a tool call, a group of them, thinking, or a turn's fold shows on its line.
struct ToolLine {
    symbol: &'static str,
    verb: String,
    target: String,
    note: Option<String>,
    failed: bool,
    running: bool,
    started_at: Option<f64>,
    /// It opens: a detail here, the rows of a group or a fold, or an agent's transcript.
    opens: bool,
    open: bool,
}

/// What opening a tool call shows: the input, then what came back, with the colours of each.
fn tool_detail(tool: &Tool, c: &Colors) -> Vec<(String, Hsla)> {
    let mut parts = Vec::new();
    if !tool.input.is_empty() {
        if tool.input_language == "diff" {
            for (index, line) in tool.input.split('\n').enumerate() {
                let color = if line.starts_with('+') {
                    c.syntax[10]
                } else if line.starts_with('-') {
                    c.syntax[11]
                } else {
                    c.text
                };
                parts.push((if index == 0 { line.to_string() } else { format!("\n{line}") }, color));
            }
        } else {
            parts.push((tool.input.clone(), c.text));
        }
    }
    if let Some(output) = tool.output.as_ref().filter(|output| !output.is_empty()) {
        if !parts.is_empty() {
            parts.push(("\n\n".into(), c.secondary));
        }
        // The first lines of long output; the rest is in the transcript on the server.
        let lines: Vec<&str> = output.split('\n').collect();
        let clipped = if lines.len() > 60 {
            format!("{}\n… {} more lines", lines[..60].join("\n"), lines.len() - 60)
        } else {
            output.clone()
        };
        parts.push((clipped, if tool.status == ToolStatus::Failed { c.danger } else { c.secondary }));
    }
    parts
}

/// The box under an open tool row, with the detail in the given font.
fn detail_box(
    row_id: &str,
    inset: f32,
    parts: Vec<(String, Hsla)>,
    family: &str,
    size: f32,
    line: f32,
    cx: &App,
) -> Div {
    let c = colors(cx);
    let text: String = parts.iter().map(|(part, _)| part.as_str()).collect();
    let runs = parts
        .iter()
        .filter(|(part, _)| !part.is_empty())
        .map(|(part, color)| run(part.len(), font(family, FontWeight::NORMAL, false), *color))
        .collect();
    div()
        .ml(px(inset + 30.))
        .mt(px(2.))
        .mb(px(8.))
        .px(px(12.))
        .py(px(10.))
        .bg(c.background_secondary)
        .rounded(px(8.))
        .child(text_box(size, line).child(crate::ui::rich_text::RichText::with_runs(
            SharedString::from(format!("{row_id}-detail")),
            text,
            runs,
        )))
}

/// A tool call or the agent's thinking: one line, which opens to show the detail. A running
/// call's title is all muted, so the band of light shows on every part of it.
fn tool_row(model: &RowModel, ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let row_id = model.row.id.clone();
    let expanded_here = ctx.expanded.contains(&row_id);
    let inset = if model.row.nested { 24. } else { 0. };
    let mut detail: Option<Div> = None;
    let line = match &model.row.kind {
        RowKind::Tool { tool } => {
            if expanded_here && !tool.agent {
                let parts = tool_detail(tool, c);
                detail = Some(detail_box(&row_id, inset, parts, theme::MONO_FONT, 11.5, theme::CODE_LINE_HEIGHT, cx));
            }
            ToolLine {
                symbol: icons::tool_symbol(tool.icon),
                verb: tool.verb.clone(),
                target: tool.target.clone(),
                note: tool.progress.clone(),
                failed: tool.status == ToolStatus::Failed,
                running: tool.status == ToolStatus::Running,
                started_at: tool.started_at,
                opens: !tool.input.is_empty() || tool.output.is_some() || tool.agent,
                open: false,
            }
        }
        RowKind::Thinking { text } => {
            if expanded_here {
                detail =
                    Some(detail_box(&row_id, inset, vec![(text.clone(), c.secondary)], theme::UI_FONT, 13., 20.5, cx));
            }
            ToolLine {
                symbol: "brain",
                verb: "Thought".into(),
                target: String::new(),
                note: None,
                failed: false,
                running: false,
                started_at: None,
                opens: !text.is_empty(),
                open: false,
            }
        }
        RowKind::Group { title, target, icon, running, failed, open, started_at } => ToolLine {
            symbol: icons::tool_symbol(icon),
            verb: title.clone(),
            target: target.clone(),
            note: None,
            failed: *failed,
            running: *running,
            started_at: *started_at,
            opens: true,
            open: *open,
        },
        RowKind::Fold { duration_ms, stopped, open } => ToolLine {
            symbol: "clock",
            verb: turn_label(*stopped, *duration_ms),
            target: String::new(),
            note: None,
            failed: false,
            running: false,
            started_at: None,
            opens: true,
            open: *open,
        },
        _ => return div().into_any_element(),
    };
    let agent = matches!(&model.row.kind, RowKind::Tool { tool } if tool.agent);
    let toggles_in_core = matches!(model.row.kind, RowKind::Group { .. } | RowKind::Fold { .. });
    let chevron_open = if toggles_in_core { line.open } else { expanded_here };
    let above = model.above();

    let mut text = if line.target.is_empty() { line.verb.clone() } else { format!("{} ", line.verb) };
    let mut runs = vec![run(text.len(), font(theme::UI_FONT, FontWeight::NORMAL, false), c.activity)];
    if !line.target.is_empty() {
        let target_color = if line.failed {
            c.danger
        } else if line.running {
            c.activity
        } else {
            c.prose
        };
        runs.push(run(line.target.len(), font(theme::MONO_FONT, FontWeight::NORMAL, false), target_color));
        text.push_str(&line.target);
    }
    if let Some(note) = &line.note {
        let note = format!("  {note}");
        runs.push(run(note.len(), font(theme::UI_FONT, FontWeight::NORMAL, false), c.tertiary));
        text.push_str(&note);
    }
    let title: AnyElement = if line.running {
        div()
            .min_w_0()
            .text_size(px(13.))
            .text_color(c.activity)
            .truncate()
            .child(
                ShimmerText::new(text)
                    .id(SharedString::from(format!("{row_id}-shine")))
                    .highlight_color(c.shimmer)
                    .truncate(),
            )
            .into_any_element()
    } else {
        div().min_w_0().text_size(px(13.)).truncate().child(StyledText::new(text).with_runs(runs)).into_any_element()
    };

    let view = ctx.view.clone();
    let store = ctx.store.clone();
    let item = model.row.item.clone();
    let click_id = row_id.clone();
    let header = div()
        .id(SharedString::from(format!("{row_id}-header")))
        .ml(px(inset - 6.))
        .mr(px(-6.))
        .mt(px(1. + above))
        .mb(px(1.))
        .h(px(TOOL_ROW_HEIGHT - 2.))
        .px(px(6.))
        .rounded(px(Radius::SMALL))
        .flex()
        .items_center()
        .child(
            div()
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .child(icons::symbol(line.symbol, 12.).text_color(c.activity)),
        )
        .child(div().ml(px(8.)).flex().items_center().min_w_0().child(title))
        .when(line.opens, |header| {
            header.child(div().ml(px(2.)).w(px(14.)).flex().justify_center().child(
                icons::symbol(if chevron_open { "chevron-down" } else { "chevron-right" }, 9.).text_color(c.tertiary),
            ))
        })
        .when_some(line.started_at, |header, started| {
            header.child(div().ml(px(6.)).text_size(px(12.)).text_color(c.tertiary).whitespace_nowrap().child(
                StyledText::new(elapsed(started)).with_runs(vec![run(
                    elapsed(started).len(),
                    digits_font(FontWeight::NORMAL),
                    c.tertiary,
                )]),
            ))
        })
        .when(line.opens, |header| {
            header.on_click(move |_, _, cx| {
                if agent {
                    let item = item.clone();
                    store.update(cx, |store, cx| {
                        store.show_agent(item, cx);
                        cx.notify();
                    });
                    return;
                }
                if toggles_in_core {
                    let _ = view.update(cx, |view, cx| view.toggle_in_core(&click_id, cx));
                    return;
                }
                let _ = view.update(cx, |view, cx| view.toggle_expanded(&click_id, cx));
            })
        });

    div().flex().flex_col().child(header).children(detail).into_any_element()
}

pub fn turn_label(stopped: bool, duration_ms: Option<u64>) -> String {
    match (stopped, duration_ms.map(duration)) {
        (true, Some(duration)) => format!("You stopped after {duration}"),
        (true, None) => "You stopped this response".into(),
        (false, Some(duration)) => format!("Worked for {duration}"),
        (false, None) => "Done".into(),
    }
}

fn error_row(model: &RowModel, message: &str, cx: &App) -> AnyElement {
    let c = colors(cx);
    div()
        .pt(px(4.))
        .pb(px(10.))
        .child(
            div()
                .bg(c.danger_background)
                .rounded(px(Radius::CARD))
                .py(px(10.))
                .pl(px(12.))
                .pr(px(14.))
                .flex()
                .gap(px(8.))
                .child(
                    div()
                        .w(px(16.))
                        .h(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icons::symbol("circle-alert", 13.).text_color(c.danger)),
                )
                .child(text_box(13., 20.5).flex_1().min_w_0().child(plain(
                    SharedString::from(format!("{}-error", model.row.id)).into(),
                    message.to_string().into(),
                    Vec::new(),
                    theme::UI_FONT,
                    c.danger,
                    cx,
                ))),
        )
        .into_any_element()
}

/// "+12 −3" in green and red.
pub fn line_counts(added: u32, removed: u32, cx: &App) -> impl IntoElement {
    let (text, runs) = crate::panel::code_view::counts_text(added, removed, colors(cx));
    div().text_size(px(11.5)).whitespace_nowrap().child(StyledText::new(text).with_runs(runs))
}

/// What a turn changed: how many files and lines, and the files under their folders. A click on
/// a file opens what the turn changed in it in the side panel; the button opens the whole diff.
fn changes_row(
    model: &RowModel,
    files: usize,
    added: u32,
    removed: u32,
    entries: &[ChangeEntry],
    ctx: &RowContext,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    let item = model.row.item.clone();
    let row_id = model.row.id.clone();
    let open_diff = ctx.store.clone();
    let open_item = item.clone();
    // The button's words end as far from the edge as the title starts.
    let button_margin = 14. - ControlSize::Regular.padding();
    div()
        .pt(px(12.))
        .pb(px(14.))
        .child(
            div()
                .bg(c.background_secondary)
                .border_1()
                .border_color(c.border)
                .rounded(px(Radius::CARD))
                .pb(px(6.))
                .child(
                    div()
                        .h(px(40.))
                        .pl(px(14.))
                        .flex()
                        .items_center()
                        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(c.text).child(
                            if files == 1 { "1 changed file".to_string() } else { format!("{files} changed files") },
                        ))
                        .child(div().ml(px(10.)).child(line_counts(added, removed, cx)))
                        .child(div().flex_1())
                        .child(
                            ActionButton::new(SharedString::from(format!("open-diff-{row_id}")), "Open diff")
                                .ghost()
                                .surface(Surface::Secondary)
                                .help("Show what this turn changed")
                                .margin(edges(6., 2., 6., button_margin.max(0.)))
                                .on_click(move |_, _, cx| {
                                    let item = open_item.clone();
                                    open_diff.update(cx, |store, cx| {
                                        store.show_diff(
                                            Some(motile_protocol::wire::DiffScope::Turn { item_id: item }),
                                            None,
                                        );
                                        cx.notify();
                                    })
                                }),
                        ),
                )
                .children(entries.iter().map(|entry| change_entry(entry, &row_id, &item, ctx, cx))),
        )
        .into_any_element()
}

/// One file a turn changed, or a folder of them, lit under the pointer.
fn change_entry(entry: &ChangeEntry, row_id: &str, item: &str, ctx: &RowContext, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let group: SharedString = format!("entry-{row_id}-{}", entry.id).into();
    let (store, view) = (ctx.store.clone(), ctx.view.clone());
    let (entry_id, path, folder, turn, toggled_row) =
        (entry.id.clone(), entry.path.clone(), entry.folder, item.to_string(), row_id.to_string());
    let symbol = if entry.folder { "folder" } else { crate::panel::state::file_symbol(&entry.path) };
    div()
        .id(group.clone())
        .group(group.clone())
        .relative()
        .h(px(26.))
        .flex()
        .items_center()
        .child(highlight_in(group, Radius::SMALL, edges(1., 6., 1., 6.), false, c.background_tertiary, cx))
        .child(
            div()
                .relative()
                .w_full()
                .pl(px(12. + entry.depth as f32 * 16.))
                .pr(px(14.))
                .flex()
                .items_center()
                .child(div().w(px(16.)).flex().child(if entry.folder {
                    icons::symbol(if entry.open { "chevron-down" } else { "chevron-right" }, 8.)
                        .text_color(c.tertiary)
                        .into_any_element()
                } else {
                    div().into_any_element()
                }))
                .child(div().w(px(24.)).flex().child(icons::symbol(symbol, 11.).text_color(c.secondary)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(theme::MONO_FONT)
                        .text_size(px(11.5))
                        .text_color(if entry.folder { c.secondary } else { c.text })
                        .truncate()
                        .child(entry.name.clone()),
                )
                .child(div().ml(px(12.)).child(line_counts(entry.added, entry.removed, cx))),
        )
        .on_click(move |_, _, cx| {
            if folder {
                let _ = view.update(cx, |view, cx| view.toggle_folder(&entry_id, &toggled_row, cx));
                return;
            }
            let (turn, path) = (turn.clone(), path.clone());
            store.update(cx, |store, cx| {
                store.show_change(turn, path);
                cx.notify();
            })
        })
}

/// Closes a turn: a way to copy the reply, when it ended and, unless its fold says so, how long
/// it took.
fn turn_end_row(
    model: &RowModel,
    stopped: bool,
    duration_ms: Option<u64>,
    folded: bool,
    at: f64,
    ctx: &RowContext,
    cx: &App,
) -> AnyElement {
    let (rows, row_id) = (ctx.rows.clone(), model.row.id.clone());
    let stamp = if !folded && (stopped || duration_ms.is_some()) {
        format!("{} · {}", stamp(at), turn_label(stopped, duration_ms))
    } else {
        stamp(at)
    };
    div()
        .h(px(TURN_END_HEIGHT))
        .child(message_meta(&model.row.id, stamp, false, "Copy reply", ctx, cx, move |cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(reply_text(&rows, &row_id)));
        }))
        .into_any_element()
}

/// The reply that ends at the row: every stretch of prose and code back to the user's message.
fn reply_text(rows: &[Arc<RowModel>], end_id: &str) -> String {
    let Some(end) = rows.iter().position(|row| row.row.id == end_id) else { return String::new() };
    let mut parts: Vec<String> = Vec::new();
    for row in rows[..end].iter().rev() {
        if row.is_user() {
            break;
        }
        if let Some(text) = row.plain_text() {
            parts.push(text);
        }
    }
    parts.reverse();
    parts.join("\n\n")
}

/// Shown under the transcript while the agent is at work or waits for an approval.
pub fn working_line(activity: &crate::models::Activity, cx: &App) -> AnyElement {
    let c = colors(cx);
    let waiting = !activity.approvals.is_empty();
    let words = if waiting {
        "Waiting for your approval".to_string()
    } else {
        let verb = if activity.compacting {
            "Compacting"
        } else if activity.thinking {
            "Thinking"
        } else {
            "Working"
        };
        match activity.started_at {
            Some(started) => format!("{verb} for {}", elapsed(started)),
            None => format!("{verb}…"),
        }
    };
    let digits = digits_font(FontWeight::NORMAL);
    div()
        .h(px(TURN_END_HEIGHT))
        .pt(px(5.))
        .text_size(px(13.))
        .text_color(c.activity)
        .font(digits)
        .child(if waiting {
            div().child(words).into_any_element()
        } else {
            ShimmerText::new(words).id("working").highlight_color(c.shimmer).into_any_element()
        })
        .into_any_element()
}

pub fn render_row(model: &RowModel, ctx: &RowContext, cx: &App) -> AnyElement {
    match &model.row.kind {
        RowKind::User { text, links, attachments, at } => {
            let files: Vec<AttachedFile> = attachments.iter().map(AttachedFile::from).collect();
            let links = prose::link_ranges(text, links);
            user_row(model, text, links, &files, *at, ctx, cx)
        }
        RowKind::Prose { .. } => prose_row(model, ctx, cx),
        RowKind::Code { language, code, .. } => code_row(model, language, code, ctx, cx),
        RowKind::Tool { .. } | RowKind::Thinking { .. } | RowKind::Group { .. } | RowKind::Fold { .. } => {
            tool_row(model, ctx, cx)
        }
        RowKind::Media { .. } => crate::media::media_row(model, ctx, cx),
        RowKind::Error { message } => error_row(model, message, cx),
        RowKind::Changes { files, added, removed, entries, .. } => {
            changes_row(model, *files, *added, *removed, entries, ctx, cx)
        }
        RowKind::TurnEnd { duration_ms, stopped, folded, at, .. } => {
            turn_end_row(model, *stopped, *duration_ms, *folded, *at, ctx, cx)
        }
        RowKind::Queued { text, attachments, status } => {
            let files: Vec<AttachedFile> = attachments.iter().map(AttachedFile::from).collect();
            queued_row(model, text, &files, status, ctx, cx)
        }
    }
}
