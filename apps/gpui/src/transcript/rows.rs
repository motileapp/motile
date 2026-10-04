//! How each kind of row is drawn. A row is as wide as the transcript's column.

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::rows::{After, ChangeEntry, RowKind, Tool};
use motile_protocol::wire::ToolStatus;

use super::model::RowModel;
use super::prose::{self, CodeBox, ParaStyle, ProseBlock, TableCell, TextPara};
use super::view::TranscriptView;
use crate::models::{AttachedFile, duration, elapsed};
use crate::store::Store;
use crate::theme::{self, Colors, colors, is_dark};
use crate::ui::{IconButton, edges, highlight, icons};

/// The room a prose row has around its text, which is what keeps two rows apart.
pub const PROSE_GAP: f32 = 9.;
pub const TOOL_ROW_HEIGHT: f32 = 28.;
pub const TURN_END_HEIGHT: f32 = 47.;
const BUBBLE_PADDING: f32 = 14.;
const CODE_HEADER: f32 = 36.;
const CODE_BOTTOM: f32 = 12.;

/// What drawing a row needs from the transcript around it.
#[derive(Clone)]
pub struct RowContext {
    pub view: WeakEntity<TranscriptView>,
    pub store: Entity<Store>,
    /// The tool calls opened here, to show their input and output.
    pub expanded: Rc<HashSet<String>>,
    /// The row whose copy button has just copied.
    pub copied: Option<String>,
    /// The rows of the transcript, for copying a reply.
    pub rows: Rc<Vec<std::sync::Arc<RowModel>>>,
}

fn inline_code_background(cx: &App) -> Hsla {
    if is_dark(cx) { hsla(0., 0., 1., 0.1) } else { hsla(0., 0., 0., 0.06) }
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

/// The runs of a paragraph: its plain text in `color`, and each styled stretch as the Mac draws
/// it, bold in the text's colour, code in the code font on a tint, links in the link colour.
fn text_runs(
    text: &str,
    runs: &[(Range<usize>, u32)],
    links: &[(Range<usize>, String)],
    color: Hsla,
    weight: FontWeight,
    heading: bool,
    cx: &App,
) -> Vec<TextRun> {
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
                font(theme::SYSTEM_FONT, if bold { FontWeight::SEMIBOLD } else { weight }, prose::is_italic(bits))
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
    let runs = text_runs(&para.text, &para.runs, &para.links, color, weight, heading, cx);
    let text = if para.text.is_empty() { SharedString::from(" ") } else { para.text.clone() };
    let runs = if para.text.is_empty() {
        vec![TextRun {
            len: 1,
            font: font(theme::SYSTEM_FONT, weight, false),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }]
    } else {
        runs
    };
    let code: Vec<Range<usize>> =
        para.runs.iter().filter(|(_, bits)| prose::is_code(*bits)).map(|(range, _)| range.clone()).collect();
    crate::ui::rich_text::RichText::with_runs(id, text, runs)
        .links(para.links.clone(), open_link)
        .tinted(code, inline_code_background(cx))
        .into_any_element()
}

/// Plain text, selectable.
fn plain(id: ElementId, text: SharedString, family: &str, color: Hsla) -> AnyElement {
    let runs = vec![TextRun {
        len: text.len(),
        font: font(family, FontWeight::NORMAL, false),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }];
    if text.is_empty() {
        return div().into_any_element();
    }
    crate::ui::rich_text::RichText::with_runs(id, text, runs).into_any_element()
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
            runs.push(TextRun {
                len: range.start - at,
                font: base.clone(),
                color: c.text,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
        let color = c.syntax.get(*colour as usize).copied().unwrap_or(c.text);
        runs.push(TextRun {
            len: range.end - range.start,
            font: base.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        at = range.end;
    }
    if at < code.len() {
        runs.push(TextRun {
            len: code.len() - at,
            font: base,
            color: c.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    crate::ui::rich_text::RichText::with_runs(id, code.clone(), runs).into_any_element()
}

/// The top of a code box: the language, and a button that copies the code.
fn code_header(
    id: &str,
    language: &str,
    code: &SharedString,
    copied: bool,
    ctx: &RowContext,
    cx: &App,
) -> impl IntoElement {
    let c = colors(cx);
    let (view, code, key) = (ctx.view.clone(), code.to_string(), id.to_string());
    div()
        .h(px(CODE_HEADER))
        .pl(px(14.))
        .pr(px(4.))
        .flex()
        .items_center()
        .child(
            div()
                .flex_1()
                .font_family(theme::MONO_FONT)
                .text_size(px(11.5))
                .text_color(c.secondary)
                .child(if language.is_empty() { "text".to_string() } else { language.to_string() }),
        )
        .child(
            IconButton::new(
                SharedString::from(format!("copy-code-{id}")),
                if copied { "checkmark" } else { "doc.on.doc" },
            )
            .help("Copy code")
            .size(28.)
            .symbol_size(14.)
            .color(c.secondary)
            .on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                let _ = view.update(cx, |view, cx| view.show_copied(key.clone(), cx));
            }),
        )
}

fn code_box(id: &str, code: &CodeBox, ctx: &RowContext, cx: &App) -> Div {
    let c = colors(cx);
    let copied = ctx.copied.as_deref() == Some(id);
    div()
        .ml(px(code.indent))
        .mb(px(12.))
        .bg(c.code_background)
        .border_1()
        .border_color(c.border)
        .rounded(px(10.))
        .overflow_hidden()
        .child(code_header(id, &code.language, &code.code, copied, ctx, cx))
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
    let RowKind::Prose { after, prose } = &model.row.kind else { return div().into_any_element() };
    let first_heading = prepared
        .blocks
        .first()
        .is_some_and(|block| matches!(block, ProseBlock::Para(TextPara { style: ParaStyle::Heading(_), .. })));
    let heading_gap = if first_heading { prose::HEADING_GAP } else { 0. };
    let above = match after {
        None => 0.,
        Some(After::Prose) => 7. + prose::BLOCK_GAP - PROSE_GAP + heading_gap,
        Some(After::Table) => prose::BLOCK_GAP - PROSE_GAP + heading_gap,
        Some(_) => heading_gap,
    };
    let _ = prose;
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
                        prose::BLOCK_GAP + 7.,
                    ),
                    ParaStyle::ListItem { .. } => {
                        (theme::PROSE_SIZE, theme::PROSE_LINE_HEIGHT, FontWeight::NORMAL, c.prose, 0., 6. + 7.)
                    }
                    ParaStyle::Quote(_) => {
                        (theme::PROSE_SIZE, theme::PROSE_LINE_HEIGHT, FontWeight::NORMAL, c.secondary, 0., 8. + 7.)
                    }
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
        .children((0..depth).map(|_| div().w(px(2.)).h_full().bg(c.strong_border)))
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

/// A message of the user: a bubble against the column's right edge, as wide as its text, with
/// the images and files it came with above the text.
fn user_row(model: &RowModel, text: &str, attachments: &[AttachedFile], ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let pending = model.row.id == "pending";
    let has_files = !attachments.is_empty();
    div()
        .py(px(14.))
        .flex()
        .justify_end()
        .child(
            div()
                .max_w(relative(0.8))
                .bg(c.bubble)
                .rounded(px(18.))
                .px(px(BUBBLE_PADDING))
                .pt(px(if has_files { BUBBLE_PADDING } else { 10. }))
                .pb(px(if text.is_empty() { BUBBLE_PADDING } else { 10. }))
                .when(pending, |bubble| bubble.opacity(0.6))
                .flex()
                .flex_col()
                .gap(px(if has_files && !text.is_empty() { 8. } else { 0. }))
                .when(has_files, |bubble| bubble.child(crate::media::attached_files(&ctx.store, attachments, cx)))
                .when(!text.is_empty(), |bubble| {
                    bubble.child(text_box(theme::PROSE_SIZE, 22.).text_color(c.text).child(plain(
                        SharedString::from(format!("{}-text", model.row.id)).into(),
                        text.to_string().into(),
                        theme::SYSTEM_FONT,
                        c.text,
                    )))
                }),
        )
        .into_any_element()
}

/// A message that waits for the agent: what it says, how it waits, and the buttons that send it
/// now or take it back. It stands where the user's messages do, outlined instead of filled.
fn queued_row(
    model: &RowModel,
    text: &str,
    attachments: &[AttachedFile],
    status: &str,
    sending: bool,
    ctx: &RowContext,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    let has_files = !attachments.is_empty();
    let message = model.row.item.clone();
    let (send, cancel) = (ctx.store.clone(), ctx.store.clone());
    let (send_id, cancel_id) = (message.clone(), message);
    let button_margin = BUBBLE_PADDING - 8.;
    let row_button =
        |id: String, title: &'static str, help: &'static str, right: f32, on_click: Box<dyn Fn(&mut App)>| {
            let group: SharedString = format!("queued-{id}").into();
            let margin = edges(4., 2., button_margin, right);
            div()
                .id(SharedString::from(id))
                .group(group.clone())
                .relative()
                .h(px(34.))
                .pt(px(margin.top))
                .pb(px(margin.bottom))
                .pl(px(margin.left))
                .pr(px(margin.right))
                .child(highlight(group, 18. - button_margin, margin, false, cx))
                .child(
                    div()
                        .relative()
                        .h_full()
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .text_size(px(12.))
                        .text_color(c.secondary)
                        .child(title),
                )
                .tooltip(crate::ui::tooltip(help))
                .on_click(move |_, _, cx| on_click(cx))
        };
    div()
        .py(px(14.))
        .flex()
        .justify_end()
        .child(
            div()
                .max_w(relative(0.8))
                .border_1()
                .border_color(c.strong_border)
                .rounded(px(18.))
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(BUBBLE_PADDING))
                        .pt(px(if has_files { BUBBLE_PADDING } else { 10. }))
                        .flex()
                        .flex_col()
                        .gap(px(if has_files && !text.is_empty() { 8. } else { 0. }))
                        .when(has_files, |bubble| {
                            bubble.child(crate::media::attached_files(&ctx.store, attachments, cx))
                        })
                        .when(!text.is_empty(), |bubble| {
                            bubble.child(text_box(theme::PROSE_SIZE, 22.).child(plain(
                                SharedString::from(format!("{}-text", model.row.id)).into(),
                                text.to_string().into(),
                                theme::SYSTEM_FONT,
                                c.prose,
                            )))
                        }),
                )
                .child(
                    div()
                        .mt(px(2.))
                        .h(px(34.))
                        .pl(px(BUBBLE_PADDING))
                        .flex()
                        .items_center()
                        .child(icons::symbol("clock", 11.).text_color(c.secondary))
                        .child(
                            div()
                                .ml(px(4.))
                                .pr(px(8.))
                                .text_size(px(12.))
                                .text_color(c.secondary)
                                .child(status.to_string()),
                        )
                        .child(div().flex_1().min_w(px(10. - BUBBLE_PADDING)))
                        .when(!sending, |foot| {
                            foot.child(row_button(
                                format!("send-now-{send_id}"),
                                "Send now",
                                "Have the agent take it at once, in the turn that runs",
                                2.,
                                Box::new(move |cx| {
                                    send.update(cx, |store, cx| {
                                        store.send_now(&send_id);
                                        cx.notify();
                                    })
                                }),
                            ))
                            .child(row_button(
                                format!("cancel-{cancel_id}"),
                                "Cancel",
                                "Take it back into the composer",
                                button_margin,
                                Box::new(move |cx| {
                                    cancel.update(cx, |store, cx| {
                                        store.take_back(&cancel_id);
                                        cx.notify();
                                    })
                                }),
                            ))
                        })
                        .when(sending, |foot| foot.pr(px(BUBBLE_PADDING))),
                ),
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

fn tool_row(model: &RowModel, ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let row_id = model.row.id.clone();
    let expanded_here = ctx.expanded.contains(&row_id);
    let mut detail: Option<Vec<(String, Hsla)>> = None;
    let line = match &model.row.kind {
        RowKind::Tool { tool } => {
            let line = ToolLine {
                symbol: icons::tool_symbol(tool.icon),
                verb: tool.verb.clone(),
                target: tool.target.clone(),
                note: tool.progress.clone(),
                failed: tool.status == ToolStatus::Failed,
                running: tool.status == ToolStatus::Running,
                started_at: tool.started_at,
                opens: !tool.input.is_empty() || tool.output.is_some() || tool.agent,
                open: false,
            };
            if expanded_here && !tool.agent {
                detail = Some(tool_detail(tool, c));
            }
            line
        }
        RowKind::Thinking { text } => {
            if expanded_here {
                detail = Some(vec![(text.clone(), c.secondary)]);
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
    let inset = if model.row.nested { 24. } else { 0. };
    let group: SharedString = format!("tool-{row_id}").into();

    let title_text = if line.target.is_empty() { line.verb.clone() } else { format!("{} {}", line.verb, line.target) };
    let title: AnyElement = if line.running {
        let words = match &line.note {
            Some(note) => format!("{title_text}  {note}"),
            None => title_text.clone(),
        };
        div()
            .min_w_0()
            .text_size(px(13.))
            .text_color(c.secondary)
            .truncate()
            .child(ShimmerText::new(words).id(SharedString::from(format!("{row_id}-shine"))).truncate())
            .into_any_element()
    } else {
        let mut text = line.verb.clone();
        let mut runs = vec![TextRun {
            len: text.len(),
            font: font(theme::SYSTEM_FONT, FontWeight::NORMAL, false),
            color: c.secondary,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        if !line.target.is_empty() {
            text.push(' ');
            runs[0].len += 1;
            let target_color = if line.failed { c.danger } else { c.prose };
            runs.push(TextRun {
                len: line.target.len(),
                font: font(theme::MONO_FONT, FontWeight::NORMAL, false),
                color: target_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            text.push_str(&line.target);
        }
        if let Some(note) = &line.note {
            let note = format!("  {note}");
            runs.push(TextRun {
                len: note.len(),
                font: font(theme::SYSTEM_FONT, FontWeight::NORMAL, false),
                color: c.tertiary,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
            text.push_str(&note);
        }
        div().min_w_0().text_size(px(13.)).truncate().child(StyledText::new(text).with_runs(runs)).into_any_element()
    };

    let view = ctx.view.clone();
    let store = ctx.store.clone();
    let item = model.row.item.clone();
    let click_id = row_id.clone();
    let header = div()
        .id(SharedString::from(format!("{row_id}-header")))
        .group(group.clone())
        .relative()
        .ml(px(inset - 6.))
        .mr(px(-6.))
        .h(px(TOOL_ROW_HEIGHT))
        .flex()
        .items_center()
        .when(line.opens, |header| header.child(highlight(group, 6., edges(1., 0., 1., 0.), false, cx)))
        .child(
            div()
                .relative()
                .w_full()
                .px(px(6.))
                .flex()
                .items_center()
                .child(
                    div()
                        .w(px(16.))
                        .flex()
                        .justify_center()
                        .child(icons::symbol(line.symbol, 12.).text_color(c.secondary)),
                )
                .child(div().ml(px(8.)).flex().items_center().min_w_0().child(title))
                .when(line.opens, |header| {
                    header.child(
                        div().ml(px(2.)).w(px(14.)).flex().justify_center().child(
                            icons::symbol(if chevron_open { "chevron.down" } else { "chevron.right" }, 9.)
                                .text_color(c.tertiary),
                        ),
                    )
                })
                .when_some(line.started_at, |header, started| {
                    header.child(div().ml(px(6.)).text_size(px(12.)).text_color(c.tertiary).child(elapsed(started)))
                }),
        )
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

    div()
        .flex()
        .flex_col()
        .child(header)
        .when_some(detail, |row, parts| {
            let text: String = parts.iter().map(|(part, _)| part.as_str()).collect();
            let runs = parts
                .iter()
                .filter(|(part, _)| !part.is_empty())
                .map(|(part, color)| TextRun {
                    len: part.len(),
                    font: font(theme::MONO_FONT, FontWeight::NORMAL, false),
                    color: *color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                })
                .collect();
            row.child(
                div()
                    .ml(px(inset + 30.))
                    .mt(px(2.))
                    .mb(px(8.))
                    .px(px(12.))
                    .py(px(10.))
                    .bg(c.code_background)
                    .rounded(px(8.))
                    .text_size(px(11.5))
                    .line_height(px(theme::CODE_LINE_HEIGHT))
                    .child(crate::ui::rich_text::RichText::with_runs(
                        SharedString::from(format!("{row_id}-detail")),
                        text,
                        runs,
                    )),
            )
        })
        .into_any_element()
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
                .rounded(px(10.))
                .py(px(10.))
                .pl(px(12.))
                .pr(px(14.))
                .flex()
                .gap(px(8.))
                .child(
                    div()
                        .h(px(16.))
                        .flex()
                        .items_center()
                        .child(icons::symbol("exclamationmark.circle", 13.).text_color(c.danger)),
                )
                .child(text_box(13., 21.).flex_1().child(plain(
                    SharedString::from(format!("{}-error", model.row.id)).into(),
                    message.to_string().into(),
                    theme::SYSTEM_FONT,
                    c.danger,
                ))),
        )
        .into_any_element()
}

/// "+12 −3" in green and red.
pub fn line_counts(added: u32, removed: u32, cx: &App) -> impl IntoElement {
    let (text, runs) = crate::panel::code_view::counts_text(added, removed, colors(cx));
    div().text_size(px(11.5)).child(StyledText::new(text).with_runs(runs))
}

/// What a turn changed: how many files and lines, and the files under their folders. A click on
/// a file shows the turn's diff with that file in view.
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
    let button_margin = 14. - 8.;
    let margin = edges(button_margin, 2., button_margin, button_margin);
    let group: SharedString = format!("open-diff-{row_id}").into();
    div()
        .pt(px(4.))
        .pb(px(14.))
        .child(
            div()
                .bg(c.code_background)
                .border_1()
                .border_color(c.border)
                .rounded(px(10.))
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
                        .child(div().ml(px(12.)).child(line_counts(added, removed, cx)))
                        .child(div().flex_1())
                        .child(
                            div()
                                .id(group.clone())
                                .group(group.clone())
                                .relative()
                                .h_full()
                                .pt(px(margin.top))
                                .pb(px(margin.bottom))
                                .pl(px(margin.left))
                                .pr(px(margin.right))
                                .child(highlight(group, 10. - button_margin, margin, false, cx))
                                .child(
                                    div()
                                        .relative()
                                        .h_full()
                                        .px(px(8.))
                                        .flex()
                                        .items_center()
                                        .text_size(px(12.))
                                        .text_color(c.secondary)
                                        .child("Open diff"),
                                )
                                .tooltip(crate::ui::tooltip("Show what this turn changed"))
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
                .children(entries.iter().map(|entry| {
                    let group: SharedString = format!("entry-{row_id}-{}", entry.id).into();
                    let (store, view) = (ctx.store.clone(), ctx.view.clone());
                    let (entry_id, path, folder, turn, toggled_row) =
                        (entry.id.clone(), entry.path.clone(), entry.folder, item.clone(), row_id.clone());
                    let symbol = if entry.folder { "folder" } else { crate::panel::state::file_symbol(&entry.path) };
                    div()
                        .id(group.clone())
                        .group(group.clone())
                        .relative()
                        .h(px(26.))
                        .flex()
                        .items_center()
                        .child(highlight(group, 6., edges(1., 6., 1., 6.), false, cx))
                        .child(
                            div()
                                .relative()
                                .w_full()
                                .pl(px(12. + entry.depth as f32 * 16.))
                                .pr(px(14.))
                                .flex()
                                .items_center()
                                .child(div().w(px(16.)).child(if entry.folder {
                                    icons::symbol(if entry.open { "chevron.down" } else { "chevron.right" }, 8.)
                                        .text_color(c.tertiary)
                                        .into_any_element()
                                } else {
                                    div().into_any_element()
                                }))
                                .child(div().w(px(24.)).child(icons::symbol(symbol, 11.).text_color(c.secondary)))
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
                                store.show_diff(
                                    Some(motile_protocol::wire::DiffScope::Turn { item_id: turn }),
                                    Some(path),
                                );
                                cx.notify();
                            })
                        })
                })),
        )
        .into_any_element()
}

/// The line that closes a turn: how long it took, and a way to copy the reply.
fn turn_end_row(
    model: &RowModel,
    stopped: bool,
    duration_ms: Option<u64>,
    folded: bool,
    ctx: &RowContext,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    let copied = ctx.copied.as_deref() == Some(model.row.id.as_str());
    let (view, rows, row_id) = (ctx.view.clone(), ctx.rows.clone(), model.row.id.clone());
    div()
        .h(px(TURN_END_HEIGHT))
        .flex()
        .flex_col()
        .child(
            div()
                .mt(px(-4.))
                .h(px(28.))
                .ml(px(if folded { -6. } else { 0. }))
                .flex()
                .items_center()
                .gap(px(4.))
                .when(!folded, |line| {
                    line.child(div().text_size(px(12.)).text_color(c.tertiary).child(turn_label(stopped, duration_ms)))
                })
                .child(
                    IconButton::new(
                        SharedString::from(format!("copy-reply-{row_id}")),
                        if copied { "checkmark" } else { "doc.on.doc" },
                    )
                    .help("Copy reply")
                    .size(28.)
                    .symbol_size(14.)
                    .color(c.secondary)
                    .on_click(move |_, _, cx| {
                        let Some(end) = rows.iter().position(|row| row.row.id == row_id) else { return };
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
                        cx.write_to_clipboard(ClipboardItem::new_string(parts.join("\n\n")));
                        let key = row_id.clone();
                        let _ = view.update(cx, |view, cx| view.show_copied(key, cx));
                    }),
                ),
        )
        .child(div().mt(px(6.)).h(px(1.)).w_full().bg(c.border))
        .into_any_element()
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
    div()
        .h(px(TURN_END_HEIGHT))
        .pt(px(5.))
        .text_size(px(13.))
        .text_color(c.secondary)
        .child(if waiting {
            div().child(words).into_any_element()
        } else {
            ShimmerText::new(words).id("working").into_any_element()
        })
        .into_any_element()
}

pub fn render_row(model: &RowModel, ctx: &RowContext, cx: &App) -> AnyElement {
    match &model.row.kind {
        RowKind::User { text, attachments, .. } => {
            let files: Vec<AttachedFile> = attachments.iter().map(AttachedFile::from).collect();
            user_row(model, text, &files, ctx, cx)
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
        RowKind::TurnEnd { duration_ms, stopped, folded, .. } => {
            turn_end_row(model, *stopped, *duration_ms, *folded, ctx, cx)
        }
        RowKind::Queued { text, attachments, status, sending } => {
            let files: Vec<AttachedFile> = attachments.iter().map(AttachedFile::from).collect();
            queued_row(model, text, &files, status, *sending, ctx, cx)
        }
    }
}
