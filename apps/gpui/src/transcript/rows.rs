//! How each kind of row is drawn, as the Mac's `RowViews.swift` draws them. A row is as wide as
//! the transcript's column.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_core::render::agents::AgentState;
use motile_core::render::rows::{AgentRow, CardPlace, ChangeEntry, HandoffLabel, RowKind, StateCount, Tool};
use motile_protocol::wire::{Agent, Ask, ToolStatus};

use super::kit::{GrowthFade, RowButton, Shimmer, Sideways, TailLine, dotted_outline};
use super::model::RowModel;
use super::prose::{self, CodeBox, ParaStyle, ProseBlock, TableCell, TextPara};
use super::view::TranscriptView;
use crate::models::{AttachedFile, duration, elapsed};
use crate::store::Store;
use crate::store::settings::SettingsSection;
use crate::theme::{self, Colors, ControlSize, Radius, Surface, colors};
use crate::ui::{ActionButton, Highlight, edges, icons};

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
const QUEUED_RADIUS: f32 = Radius::XL;
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
    /// The rows whose time and copy button are fading in or out: whether they show, and since when.
    pub meta_fades: Rc<HashMap<String, (bool, Instant)>>,
    /// Prose rows that grew while a reply streamed, and when.
    pub grown: Rc<HashMap<String, Instant>>,
    /// The rows of the transcript, for copying a reply.
    pub rows: Rc<Vec<Arc<RowModel>>>,
    /// How wide the column of rows is.
    pub width: f32,
    /// It is the transcript of an agent the thread started, whose turns' changes don't open.
    pub agent: bool,
    /// The thread's server was removed from the account, so what only it had can't be fetched.
    pub removed: bool,
}

/// What the text of a row is known by, so that a selection keeps to the row.
pub fn row_key(row_id: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    row_id.hash(&mut hasher);
    hasher.finish()
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

/// The time of day, with the date before it when that isn't today, as the user's region writes
/// them: "2:03 PM", "Oct 14, 2026 at 2:03 PM".
pub fn stamp(at: f64) -> String {
    crate::format::stamp(at)
}

/// How a paragraph's plain text is set: its font, size, colour and weight, and whether it is a
/// heading.
struct Plain<'a> {
    family: &'a str,
    size: f32,
    color: Hsla,
    weight: FontWeight,
    heading: bool,
}

/// The runs of a paragraph: its plain text in `color`, and each styled stretch as the Mac draws
/// it, bold in the text's colour, code in the code font on a tint, links in the link colour.
fn text_runs(
    text: &str,
    runs: &[(Range<usize>, u32)],
    links: &[(Range<usize>, String)],
    plain: Plain,
    cx: &App,
) -> Vec<TextRun> {
    let Plain { family, size, color, weight, heading } = plain;
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
                crate::fonts::inline_code(size, bold || heading)
            } else {
                font(family, if bold { FontWeight::SEMIBOLD } else { weight }, prose::is_italic(bits))
            };
            let run_color = if link {
                c.primary
            } else if code || bold {
                c.foreground
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

/// How a paragraph of prose is drawn: its size, weight and colour, and the row it is in.
struct ParaLook {
    size: f32,
    weight: FontWeight,
    color: Hsla,
    heading: bool,
    key: u64,
}

/// A paragraph of prose, selectable, its links opening in the browser.
fn paragraph(id: ElementId, para: &TextPara, look: ParaLook, cx: &App) -> AnyElement {
    let c = colors(cx);
    let ParaLook { size, weight, color, heading, key } = look;
    if para.text.is_empty() {
        let runs = vec![run(1, font(theme::UI_FONT, weight, false), color)];
        return crate::ui::rich_text::RichText::with_runs(id, " ", runs).row(key).into_any_element();
    }
    let plain = Plain { family: theme::UI_FONT, size, color, weight, heading };
    let runs = text_runs(&para.text, &para.runs, &para.links, plain, cx);
    let code: Vec<Range<usize>> =
        para.runs.iter().filter(|(_, bits)| prose::is_code(*bits)).map(|(range, _)| range.clone()).collect();
    crate::ui::rich_text::RichText::with_runs(id, para.text.clone(), runs)
        .row(key)
        .links(para.links.clone(), open_link)
        .tinted(code, c.background_accent)
        .tint_size(px(crate::fonts::inline_code_size(size)))
        .into_any_element()
}

/// The room the Mac keeps after each line of a message, an error or a thought.
const PLAIN_PARAGRAPH_GAP: f32 = 6.;

/// Plain text, selectable, with its web addresses clickable, the names of its skills on chips,
/// and room after each of its lines as the Mac keeps after a paragraph. Each line is text of its
/// own, and the blank lines after it go with it, so that a copied selection has them.
fn plain(
    id: &str,
    text: &str,
    links: &[(Range<usize>, String)],
    skills: &[Range<usize>],
    color: Hsla,
    key: u64,
    cx: &App,
) -> Div {
    let column = div().flex().flex_col();
    if text.is_empty() {
        return column;
    }
    let parts = plain_parts(text);
    let count = parts.len();
    column.children(parts.into_iter().enumerate().map(|(index, part)| {
        let shift = |range: &Range<usize>| {
            let start = range.start.max(part.start) - part.start;
            let end = range.end.min(part.end) - part.start;
            start..end
        };
        let inside = |range: &Range<usize>| range.start < part.end && range.end > part.start;
        let links: Vec<(Range<usize>, String)> =
            links.iter().filter(|(range, _)| inside(range)).map(|(range, url)| (shift(range), url.clone())).collect();
        let skills: Vec<Range<usize>> = skills.iter().filter(|range| inside(range)).map(shift).collect();
        let words: SharedString = text[part.clone()].to_string().into();
        let breaks = 1 + words.len() - words.trim_end_matches('\n').len();
        let plain = Plain { family: theme::UI_FONT, size: 0., color, weight: FontWeight::NORMAL, heading: false };
        let runs = with_skill_names(text_runs(&words, &[], &links, plain, cx), &skills, cx);
        div().when(index + 1 < count, |line| line.mb(px(PLAIN_PARAGRAPH_GAP * breaks as f32))).child(
            crate::ui::rich_text::RichText::with_runs(SharedString::from(format!("{id}-{index}")), words, runs)
                .row(key)
                .links(links, open_link)
                .skills(skills),
        )
    }))
}

/// The runs with the names of skills in the skill colour, as a chip has them.
fn with_skill_names(runs: Vec<TextRun>, names: &[Range<usize>], cx: &App) -> Vec<TextRun> {
    if names.is_empty() {
        return runs;
    }
    let skill = colors(cx).skill;
    let mut split = Vec::new();
    let mut at = 0;
    for run in runs {
        let end = at + run.len;
        let mut cuts = vec![at, end];
        cuts.extend(names.iter().flat_map(|name| [name.start, name.end]).filter(|cut| *cut > at && *cut < end));
        cuts.sort_unstable();
        for pair in cuts.windows(2) {
            let mut piece = run.clone();
            piece.len = pair[1] - pair[0];
            if names.iter().any(|name| name.start <= pair[0] && name.end >= pair[1]) {
                piece.font.weight = FontWeight::MEDIUM;
                piece.color = skill;
            }
            split.push(piece);
        }
        at = end;
    }
    split
}

/// A message's text as it is drawn, with its links and the labels of its skills as bytes of it.
struct Shown {
    text: String,
    links: Vec<(Range<usize>, String)>,
    labels: Vec<Range<usize>>,
}

/// Each skill's `$name` is its label, which no line breaks, between the room its chip takes. The
/// links move with the text.
fn with_skill_chips(message: Message, window: &mut Window) -> Shown {
    let Message { text, links, skills } = message;
    if skills.is_empty() {
        return Shown { text: text.to_string(), links, labels: Vec::new() };
    }
    let (before, after) = crate::ui::skill_chip::room();
    let (lead, trail) = (spacer(before, window), spacer(after, window));
    let mut shown = String::new();
    let mut labels = Vec::new();
    let mut shifts: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    for (range, label) in &skills {
        if range.start < at || range.end > text.len() {
            continue;
        }
        shown.push_str(&text[at..range.start]);
        shown.push_str(&lead);
        let start = shown.len();
        shown.push_str(&label.replace(' ', "\u{a0}"));
        labels.push(start..shown.len());
        shown.push_str(&trail);
        at = range.end;
        shifts.push((at, shown.len()));
    }
    shown.push_str(&text[at..]);
    let moved = |offset: usize| {
        shifts.iter().rev().find(|(end, _)| *end <= offset).map_or(offset, |(end, shown)| shown + offset - end)
    };
    let links = links
        .into_iter()
        .filter(|(link, _)| !skills.iter().any(|(skill, _)| skill.start < link.end && link.start < skill.end))
        .map(|(link, url)| (moved(link.start)..moved(link.end), url))
        .collect();
    Shown { text: shown, links, labels }
}

thread_local! {
    static SPACERS: std::cell::RefCell<HashMap<u32, String>> = std::cell::RefCell::default();
}

/// Spaces that no line breaks, as near `width` wide in a message's text as they come, short of
/// it by no more than the room a chip keeps from the text.
fn spacer(width: f32, window: &mut Window) -> String {
    if let Some(kept) = SPACERS.with_borrow(|kept| kept.get(&width.to_bits()).cloned()) {
        return kept;
    }
    let (wide, narrow) = ('\u{a0}', '\u{202f}');
    let measure = |space: char, window: &mut Window| {
        let text = SharedString::from(space.to_string());
        let runs = [run(text.len(), font(theme::UI_FONT, FontWeight::NORMAL, false), Hsla::default())];
        f32::from(window.text_system().shape_line(text, px(theme::PROSE_SIZE), &runs, None).width)
    };
    let (wide_width, narrow_width) = (measure(wide, window), measure(narrow, window));
    let fits = (0..=8usize)
        .flat_map(|wides| (0..=16usize).map(move |narrows| (wides, narrows)))
        .map(|(wides, narrows)| (wides as f32 * wide_width + narrows as f32 * narrow_width, wides, narrows))
        .filter(|(room, ..)| *room >= width - crate::ui::skill_chip::MARGIN)
        .min_by(|a, b| (a.0 - width).abs().total_cmp(&(b.0 - width).abs()).then((a.1 + a.2).cmp(&(b.1 + b.2))));
    let spaces = fits.map_or(String::new(), |(_, wides, narrows)| {
        std::iter::repeat_n(wide, wides).chain(std::iter::repeat_n(narrow, narrows)).collect()
    });
    SPACERS.with_borrow_mut(|kept| kept.insert(width.to_bits(), spaces.clone()));
    spaces
}

/// The lines of plain text, each with the blank lines after it but the last.
fn plain_parts(text: &str) -> Vec<Range<usize>> {
    let mut parts: Vec<Range<usize>> = Vec::new();
    let mut start = 0;
    for (at, _) in text.match_indices('\n') {
        let blank = at == start;
        match parts.last_mut() {
            Some(last) if blank && last.end + 1 == at => last.end = at,
            _ if blank => {}
            _ => parts.push(start..at),
        }
        start = at + 1;
    }
    if start < text.len() || parts.is_empty() {
        parts.push(start..text.len());
    }
    parts
}

/// Code with its colours, selectable.
fn code_text(
    id: ElementId,
    code: &SharedString,
    spans: Option<&Vec<(Range<usize>, u32)>>,
    key: u64,
    c: &Colors,
) -> AnyElement {
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
            runs.push(run(range.start - at, base.clone(), c.foreground));
        }
        let color = c.syntax.get(*colour as usize).copied().unwrap_or(c.foreground);
        runs.push(run(range.end - range.start, base.clone(), color));
        at = range.end;
    }
    if at < code.len() {
        runs.push(run(code.len() - at, base, c.foreground));
    }
    crate::ui::rich_text::RichText::with_runs(id, code.clone(), runs).row(key).into_any_element()
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
                .text_color(c.muted_foreground)
                .truncate()
                .child(if language.is_empty() { "text".to_string() } else { language.to_string() }),
        )
        .child(copy_button(id.to_string(), "Copy code", ctx.copied(id), Surface::Secondary, ctx, move |cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(code.clone()))
        }))
}

/// A box of code with its language and a copy button above it. Code in a reply's prose wraps its
/// long lines as the Mac's text does; a code row of its own keeps them and moves sideways.
fn code_box(id: &str, code: &CodeBox, wraps: bool, key: u64, ctx: &RowContext, cx: &App) -> Div {
    let c = colors(cx);
    let text = code_text(SharedString::from(format!("{id}-code")).into(), &code.code, code.spans.as_ref(), key, c);
    let body =
        div().px(px(14.)).pb(px(CODE_BOTTOM)).text_size(px(theme::CODE_SIZE)).line_height(px(theme::CODE_LINE_HEIGHT));
    div()
        .ml(px(code.indent))
        .mb(px(12.))
        .bg(c.background_secondary)
        .border_1()
        .border_color(c.border)
        .rounded(px(Radius::LG))
        .overflow_hidden()
        .child(code_header(id, &code.language, &code.code, ctx, cx))
        .map(|block| match wraps {
            true => block.child(body.child(text)),
            false => block
                .child(Sideways::new(SharedString::from(format!("{id}-scroll")), body.whitespace_nowrap().child(text))),
        })
}

/// A reply's prose: its paragraphs as the Mac sets them.
fn prose_row(model: &RowModel, ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let Some(prepared) = &model.prose else { return div().into_any_element() };
    let above = model.above();
    let row_id = model.row.id.clone();
    let count = prepared.blocks.len();
    let mut column = div().flex().flex_col();
    let mut code_index = 0;
    let key = row_key(&row_id);
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
                        c.foreground,
                        if index == 0 { 0. } else { prose::HEADING_GAP },
                        8. + 3.,
                    ),
                    ParaStyle::Body => (
                        theme::PROSE_SIZE,
                        theme::PROSE_LINE_HEIGHT,
                        FontWeight::NORMAL,
                        c.muted_foreground,
                        0.,
                        prose::BLOCK_GAP + prose::LINE_SPACING,
                    ),
                    ParaStyle::ListItem { .. } => (
                        theme::PROSE_SIZE,
                        theme::PROSE_LINE_HEIGHT,
                        FontWeight::NORMAL,
                        c.muted_foreground,
                        0.,
                        6. + prose::LINE_SPACING,
                    ),
                    ParaStyle::Quote(_) => (
                        theme::PROSE_SIZE,
                        theme::PROSE_LINE_HEIGHT,
                        FontWeight::NORMAL,
                        c.muted_foreground,
                        0.,
                        8. + prose::LINE_SPACING,
                    ),
                };
                let heading = matches!(para.style, ParaStyle::Heading(_));
                let text = paragraph(id, para, ParaLook { size, weight, color, heading, key }, cx);
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
                let code_id = format!("{row_id}-code-{code_index}");
                code_index += 1;
                code_box(&code_id, code, true, key, ctx, cx).into_any_element()
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
                table(&row_id, index, *columns, rows, *extra_above, last, key, cx).into_any_element()
            }
        };
        column = column.child(element);
    }
    let grown = ctx.grown.get(&row_id).copied();
    div()
        .pt(px(3. + above))
        .pb(px(PROSE_GAP))
        .child(GrowthFade::new(SharedString::from(format!("growth-{row_id}")), grown, colors(cx).background, column))
        .into_any_element()
}

fn quote_bars(depth: u8, c: &Colors) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(1.))
        .flex()
        .gap(px(12.))
        .children((0..depth).map(|_| div().w(px(2.)).h_full().bg(c.border)))
}

/// A table as the Mac's text lays one out: each column as wide as its widest cell while they fit,
/// narrower ones whole when they don't, and a line under every cell.
#[allow(clippy::too_many_arguments)]
fn table(
    row_id: &str,
    index: usize,
    columns: usize,
    rows: &[Vec<TableCell>],
    extra_above: f32,
    last: bool,
    key: u64,
    cx: &App,
) -> impl IntoElement {
    let c = colors(cx);
    let cells = rows.iter().enumerate().flat_map(|(row_index, cells)| {
        (0..columns).map(move |column| {
            let cell = cells.get(column);
            div().min_w_0().px(px(10.)).pt(px(6.)).pb(px(6. + 3.)).border_b_1().border_color(c.border).when_some(
                cell,
                |slot, cell| {
                    let weight = if cell.header { FontWeight::SEMIBOLD } else { FontWeight::NORMAL };
                    let color = if cell.header { c.foreground } else { c.muted_foreground };
                    let id = SharedString::from(format!("{row_id}-cell-{index}-{row_index}-{column}"));
                    let look = ParaLook { size: 13., weight, color, heading: cell.header, key };
                    slot.child(
                        text_box(13., 18.6)
                            .text_color(color)
                            .when(cell.align == 1, |text| text.text_center())
                            .when(cell.align == 2, |text| text.text_right())
                            .child(paragraph(id.into(), &cell.para, look, cx)),
                    )
                },
            )
        })
    });
    div()
        .pt(px(extra_above))
        .when(!last, |table| table.pb(px(prose::BLOCK_GAP)))
        .child(div().grid().grid_cols_max_content(columns.max(1) as u16).children(cells))
}

/// Under a message or a reply: when it was sent and a button that copies it, at the side the
/// message is on. It is only seen while the pointer is over its message, fading in and out, and
/// the check mark that says it was copied stays to be seen when the pointer has left.
fn message_meta(
    row_id: &str,
    stamp: String,
    trailing: bool,
    help: &'static str,
    ctx: &RowContext,
    cx: &App,
    copy: impl Fn(&mut App) + 'static,
) -> AnyElement {
    let c = colors(cx);
    let copied = ctx.copied(row_id);
    let shown = ctx.meta_shown(row_id) || copied;
    let time =
        div().px(px(2.)).text_size(px(12.)).text_color(c.muted_stronger_foreground).whitespace_nowrap().child(stamp);
    let button = copy_button(row_id.to_string(), help, copied, Surface::Background, ctx, copy);
    let meta = div().h(px(META_HEIGHT)).w_full().flex().items_center();
    let meta = if trailing { meta.justify_end().child(time).child(button) } else { meta.child(button).child(time) };
    let fade = ctx.meta_fades.get(row_id).copied().filter(|_| !copied);
    let Some((showing, at)) = fade else { return meta.opacity(if shown { 1. } else { 0. }).into_any_element() };
    let left = META_FADE.saturating_sub(at.elapsed());
    let start = 1. - left.as_secs_f32() / META_FADE.as_secs_f32();
    meta.with_animation(
        SharedString::from(format!("meta-{row_id}-{showing}-{at:?}")),
        Animation::new(left.max(Duration::from_millis(1))),
        move |meta, delta| {
            let progress = start + (1. - start) * delta;
            meta.opacity(if showing { progress } else { 1. - progress })
        },
    )
    .into_any_element()
}

/// How long the time and copy button of a message take to show and to go.
pub const META_FADE: Duration = Duration::from_millis(120);

/// What a bubble shows of a message: its text, its links and its skills.
struct Message<'a> {
    text: &'a str,
    links: Vec<(Range<usize>, String)>,
    skills: Vec<(Range<usize>, String)>,
}

/// The message and the files of a bubble, as wide as they are up to the widest a bubble may be.
fn bubble_content(
    model: &RowModel,
    message: Message,
    attachments: &[AttachedFile],
    color: Hsla,
    ctx: &RowContext,
    window: &mut Window,
    cx: &App,
) -> Div {
    let text = message.text;
    let shown = with_skill_chips(message, window);
    let has_files = !attachments.is_empty();
    div()
        .px(px(BUBBLE_PADDING))
        .pt(px(if has_files { BUBBLE_PADDING } else { 10. }))
        .flex()
        .flex_col()
        .gap(px(if has_files && !text.is_empty() { 8. } else { 0. }))
        .when(has_files, |bubble| bubble.child(crate::media::attached_files(&ctx.store, attachments, cx)))
        .when(!text.is_empty(), |bubble| {
            let id = format!("{}-text", model.row.id);
            bubble.child(text_box(theme::PROSE_SIZE, 22.).text_color(color).child(plain(
                &id,
                &shown.text,
                &shown.links,
                &shown.labels,
                color,
                row_key(&model.row.id),
                cx,
            )))
        })
}

/// A message of the user: a bubble against the column's right edge, as wide as its text, with
/// the images and files it came with above the text, and when it was sent under it.
fn user_row(
    model: &RowModel,
    message: Message,
    attachments: &[AttachedFile],
    at: f64,
    ctx: &RowContext,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    let text = message.text;
    let copied_text = text.to_string();
    div()
        .pt(px(14.))
        .pb(px(25.))
        .flex()
        .flex_col()
        .items_end()
        .child(
            bubble_content(model, message, attachments, c.foreground, ctx, window, cx)
                .max_w(relative(0.8))
                .min_w(px(12. + BUBBLE_PADDING * 2.))
                .pb(px(if text.is_empty() { BUBBLE_PADDING } else { 10. }))
                .bg(c.background_secondary)
                .rounded(px(BUBBLE_RADIUS))
                .when(model.is_pending(), |bubble| bubble.opacity(0.6)),
        )
        .child(div().w_full().mt(px(4.)).child(message_meta(
            &model.row.id,
            stamp(at),
            true,
            "Copy message",
            ctx,
            cx,
            move |cx| cx.write_to_clipboard(ClipboardItem::new_string(copied_text.clone())),
        )))
        .into_any_element()
}

/// A message that waits for the agent: what it says, how it waits, and the buttons that steer
/// the turn with it or take it back into the composer. It stands where the user's messages do,
/// dotted around instead of filled.
fn queued_row(
    model: &RowModel,
    shown: Message,
    attachments: &[AttachedFile],
    status: &str,
    ctx: &RowContext,
    window: &mut Window,
    cx: &App,
) -> AnyElement {
    let c = colors(cx);
    // The buttons are this far from the bubble's right and bottom, this far from the message,
    // and a click this far above them still presses them.
    let (margin, gap, reach) = (10., 18., 4.);
    let button_height = RowButton::METRICS.height();
    let pending = model.is_pending();
    let message = model.row.item.clone();
    let (steer, edit) = (ctx.store.clone(), ctx.store.clone());
    let (steer_id, edit_id) = (message.clone(), message.clone());
    let foot = div()
        .pt(px(gap - reach))
        .pl(px(BUBBLE_PADDING))
        .flex()
        .items_end()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .mb(px(margin))
                .h(px(button_height))
                .pr(px(6.))
                .flex()
                .items_center()
                .child(
                    div()
                        .w(px(14.))
                        .flex_none()
                        .flex()
                        .child(icons::symbol("clock", 11.).text_color(c.muted_foreground)),
                )
                .child(
                    div()
                        .ml(px(4.))
                        .min_w_0()
                        .text_size(px(12.))
                        .text_color(c.muted_foreground)
                        .truncate()
                        .child(status.to_string()),
                ),
        )
        .child(
            RowButton::new(SharedString::from(format!("steer-{message}")), "Steer", "Send as a steer instead")
                .bordered()
                .radius(QUEUED_RADIUS - margin)
                .height(button_height + reach + margin)
                .insets(edges(reach, 3., margin, 3.))
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
            RowButton::new(SharedString::from(format!("edit-{message}")), "Edit", "Edit in the composer")
                .bordered()
                .radius(QUEUED_RADIUS - margin)
                .height(button_height + reach + margin)
                .insets(edges(reach, 3., margin, margin))
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
                .relative()
                .max_w(relative(0.8))
                .rounded(px(QUEUED_RADIUS))
                .flex()
                .flex_col()
                .child(dotted_outline(QUEUED_RADIUS, c.border))
                .child(bubble_content(model, shown, attachments, c.muted_foreground, ctx, window, cx))
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
    let key = row_key(&model.row.id);
    div()
        .pt(px(4.))
        .pb(px(10.))
        .child(code_box(&model.row.id, &code, false, key, ctx, cx).mb(px(0.)))
        .into_any_element()
}

/// What a row of a tool call, a group of them, thinking, or a turn's fold shows on its line.
struct ToolLine {
    symbol: &'static str,
    verb: String,
    target: String,
    failed: bool,
    running: bool,
    started_at: Option<f64>,
    /// It opens: a detail here, or the rows of a group or a fold.
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
                    c.foreground
                };
                parts.push((if index == 0 { line.to_string() } else { format!("\n{line}") }, color));
            }
        } else {
            parts.push((tool.input.clone(), c.foreground));
        }
    }
    if let Some(output) = tool.output.as_ref().filter(|output| !output.is_empty()) {
        if !parts.is_empty() {
            parts.push(("\n\n".into(), c.muted_foreground));
        }
        // The first lines of long output; the rest is in the transcript on the server.
        let lines: Vec<&str> = output.split('\n').collect();
        let clipped = if lines.len() > 60 {
            format!("{}\n… {} more lines", lines[..60].join("\n"), lines.len() - 60)
        } else {
            output.clone()
        };
        parts.push((clipped, if tool.status == ToolStatus::Failed { c.destructive } else { c.muted_foreground }));
    }
    parts
}

/// The box under an open tool row, with its detail.
fn detail_box(inset: f32, detail: impl IntoElement, cx: &App) -> Div {
    let c = colors(cx);
    div()
        .ml(px(inset + 30.))
        .mt(px(3.))
        .mb(px(6.))
        .px(px(12.))
        .py(px(10.))
        .bg(c.background_secondary)
        .rounded(px(8.))
        .child(detail)
}

/// A tool call's input and output, in the code font.
fn tool_detail_text(row_id: &str, parts: Vec<(String, Hsla)>, key: u64) -> Div {
    let text: String = parts.iter().map(|(part, _)| part.as_str()).collect();
    let runs = parts
        .iter()
        .filter(|(part, _)| !part.is_empty())
        .map(|(part, color)| run(part.len(), font(theme::MONO_FONT, FontWeight::NORMAL, false), *color))
        .collect();
    text_box(11.5, theme::CODE_LINE_HEIGHT).child(
        crate::ui::rich_text::RichText::with_runs(SharedString::from(format!("{row_id}-detail")), text, runs).row(key),
    )
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
            if expanded_here {
                let parts = tool_detail(tool, c);
                detail = Some(detail_box(inset, tool_detail_text(&row_id, parts, row_key(&row_id)), cx));
            }
            ToolLine {
                symbol: icons::tool_symbol(tool.icon),
                verb: tool.verb.clone(),
                target: tool.target.clone(),
                failed: tool.status == ToolStatus::Failed,
                running: tool.status == ToolStatus::Running,
                started_at: tool.started_at,
                opens: !tool.input.is_empty() || tool.output.is_some(),
                open: false,
            }
        }
        RowKind::Thinking { text } => {
            if expanded_here {
                let id = format!("{row_id}-detail");
                let thought = plain(&id, text, &[], &[], c.muted_foreground, row_key(&row_id), cx);
                detail = Some(detail_box(inset, text_box(13., 20.5).text_color(c.muted_foreground).child(thought), cx));
            }
            ToolLine {
                symbol: "brain",
                verb: "Thought".into(),
                target: String::new(),
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
            failed: false,
            running: false,
            started_at: None,
            opens: true,
            open: *open,
        },
        _ => return div().into_any_element(),
    };
    let toggles_in_core = matches!(model.row.kind, RowKind::Group { .. } | RowKind::Fold { .. });
    let chevron_open = if toggles_in_core { line.open } else { expanded_here };
    let above = model.above();

    let mut text = if line.target.is_empty() { line.verb.clone() } else { format!("{} ", line.verb) };
    let mut runs = vec![run(text.len(), font(theme::UI_FONT, FontWeight::NORMAL, false), c.muted_stronger_foreground)];
    if !line.target.is_empty() {
        let target_color = if line.failed {
            c.destructive
        } else if line.running {
            c.muted_stronger_foreground
        } else {
            c.muted_foreground
        };
        runs.push(run(line.target.len(), crate::fonts::inline_code(13., false), target_color));
        text.push_str(&line.target);
    }
    let title = div().min_w_0().pr(px(4.)).text_size(px(13.)).overflow_hidden();
    let title: AnyElement = if line.running {
        title
            .child(Shimmer::new(SharedString::from(format!("{row_id}-shine")), text, runs, c.foreground))
            .into_any_element()
    } else {
        title.child(TailLine::new(text, runs)).into_any_element()
    };

    let view = ctx.view.clone();
    let click_id = row_id.clone();
    // The header reaches past the row's sides by as much room as its icon has above it, so that
    // what it lights up stands out while the icon lines up with the text. It ends as far past the
    // last thing on it as it starts before the icon, and a point further past the chevron.
    let bleed = (TOOL_ROW_HEIGHT - 2. - 16.) / 2.;
    let elapsed_shown = line.started_at.is_some();
    let end = if elapsed_shown {
        bleed
    } else if line.opens {
        5.5 + bleed - 7.
    } else {
        bleed
    };
    let header = div()
        .id(SharedString::from(format!("{row_id}-header")))
        .flex_none()
        .max_w_full()
        .ml(px(inset - bleed))
        .mt(px(1. + above))
        .mb(px(1.))
        .h(px(TOOL_ROW_HEIGHT - 2.))
        .pl(px(bleed))
        .pr(px(end))
        .rounded(px(Radius::SM))
        .when(line.opens, |header| header.hover(|header| header.bg(c.background_accent_larger)))
        .flex()
        .items_center()
        .child(
            div()
                .size(px(16.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(icons::symbol(line.symbol, 12.).text_color(c.muted_stronger_foreground)),
        )
        .child(div().ml(px(8.)).flex().items_center().min_w_0().child(title))
        .when(line.opens || elapsed_shown, |header| {
            header.child(div().ml(px(2.)).w(px(14.)).flex_none().flex().justify_center().when(line.opens, |chevron| {
                chevron.child(
                    icons::symbol(if chevron_open { "chevron-down" } else { "chevron-right" }, 9.)
                        .text_color(c.muted_stronger_foreground),
                )
            }))
        })
        .when_some(line.started_at, |header, started| {
            header.child(
                div()
                    .ml(px(6.))
                    .flex_none()
                    .text_size(px(12.))
                    .text_color(c.muted_stronger_foreground)
                    .whitespace_nowrap()
                    .child(StyledText::new(elapsed(started)).with_runs(vec![run(
                        elapsed(started).len(),
                        digits_font(FontWeight::NORMAL),
                        c.muted_stronger_foreground,
                    )])),
            )
        })
        .when(line.opens, |header| {
            header.on_click(move |_, _, cx| {
                if toggles_in_core {
                    let _ = view.update(cx, |view, cx| view.toggle_in_core(&click_id, cx));
                    return;
                }
                let _ = view.update(cx, |view, cx| view.toggle_expanded(&click_id, cx));
            })
        });

    div().flex().flex_col().child(header.self_start()).children(detail).into_any_element()
}

const AGENT_ROW_HEIGHT: f32 = 50.;
/// How far a row in a group's card stands in from the card's sides.
const CARD_PADDING: f32 = 8.;
/// How far the light of an agent's row reaches past its sides, so that the avatar lines up with
/// the text.
const AGENT_BLEED: f32 = 6.;
/// How far the light of an agent's row stands in from its top and bottom, half the room between
/// two lights.
const LIGHT_GAP: f32 = 2.;
/// How far in from the card's border a row in it lights, on every side.
const CARD_LIGHT: f32 = 4.;
/// The room above a group's card and under it, which its border stands in from by what the light
/// leaves at the card's ends.
const CARD_ABOVE: f32 = 4. + CARD_LIGHT - LIGHT_GAP;
const CARD_BELOW: f32 = 10. + CARD_LIGHT - LIGHT_GAP;

/// What the second line of an agent's row says: what it runs on, then what it does or said.
fn agent_line(agent: &AgentRow) -> String {
    let said = if agent.detail.is_empty() && agent.runs_on.is_empty() { &agent.label } else { &agent.detail };
    [&agent.runs_on, said].into_iter().filter(|part| !part.is_empty()).cloned().collect::<Vec<_>>().join(" · ")
}

/// When the thread's agent was told, for what it was told; else how long the agent has worked
/// while it works, or worked in all.
fn agent_time(agent: &AgentRow) -> String {
    if let Some(at) = agent.at {
        return stamp(at);
    }
    if matches!(agent.state, AgentState::Working | AgentState::Waiting | AgentState::Limited) {
        return elapsed(agent.started_at);
    }
    agent.duration_ms.map(duration).unwrap_or_default()
}

/// An agent's row and a group's: the avatar, two lines of text, then the time and the chevron.
fn agent_line_box(avatar: impl IntoElement, title: Div, line: Div, time: String, open: bool, cx: &App) -> Div {
    let c = colors(cx);
    let time = (!time.is_empty()).then(|| {
        let runs = vec![run(time.len(), digits_font(FontWeight::NORMAL), c.muted_stronger_foreground)];
        div().pr(px(4.)).text_size(px(12.)).whitespace_nowrap().child(StyledText::new(time).with_runs(runs))
    });
    div()
        .relative()
        .h(px(AGENT_ROW_HEIGHT))
        .flex()
        .items_center()
        .child(avatar)
        .child(
            div()
                .ml(px(10.))
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(title.h(px(18.)).line_height(px(18.)).flex().items_center().overflow_hidden())
                .child(line.h(px(16.)).line_height(px(16.)).text_size(px(12.)).truncate()),
        )
        .child(div().ml(px(12.)).flex_none().children(time))
        .child(
            div().ml(px(6.)).w(px(14.)).h(px(16.)).flex_none().flex().items_center().justify_center().child(
                icons::symbol(if open { "chevron-down" } else { "chevron-right" }, 9.)
                    .text_color(c.muted_stronger_foreground),
            ),
        )
}

/// An agent the thread's agent started, or what it was told of one: its avatar, its title and
/// word, then what it runs on and does or said, and how long it worked. A click opens it: its
/// thread, or what it did. In the card of an open group it lies on the card, of which each row
/// draws its own part.
fn agent_row(model: &RowModel, agent: &AgentRow, ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let group: SharedString = format!("{}-agent", model.row.id).into();
    let place = agent.card;
    let inset = if place.is_some() { CARD_PADDING } else { 0. };
    let above = if place == Some(CardPlace::First) { CARD_ABOVE } else { 0. };
    let below = if place == Some(CardPlace::Last) { CARD_BELOW } else { 0. };
    let overhang = Radius::LG + 1.;
    let ends = CARD_LIGHT - LIGHT_GAP;
    let card = place.map(|place| {
        let (top, bottom) = match place {
            CardPlace::First => (CARD_ABOVE - ends, -overhang),
            CardPlace::Middle => (-overhang, -overhang),
            CardPlace::Last => (-overhang, CARD_BELOW - ends),
        };
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(px(top))
            .bottom(px(bottom))
            .bg(c.card)
            .border_1()
            .border_color(c.border_card)
            .rounded(px(Radius::LG))
    });
    // In a card the light keeps as far from the card's border as from the next row's light, in the
    // card's accent.
    let side = if place.is_some() { CARD_LIGHT } else { -AGENT_BLEED };
    let light = Highlight::new().radius(Radius::MD).inset(edges(above + LIGHT_GAP, side, below + LIGHT_GAP, side));
    let light = if place.is_some() { light.fill(c.card_accent) } else { light };
    let avatar = crate::ui::AgentAvatar::new(agent.agent).state(agent.state);
    let label_color =
        if agent.state.flagged() { crate::ui::agent_avatar::state_color(agent.state, c) } else { c.muted_foreground };
    let title = div()
        .gap(px(6.))
        .child(
            div()
                .flex_shrink_0()
                .max_w_full()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(c.foreground)
                .truncate()
                .child(agent.title.clone()),
        )
        .when_some(agent.role.clone(), |title, role| {
            title.child(
                div().min_w_0().text_size(px(11.)).text_color(c.muted_stronger_foreground).truncate().child(role),
            )
        })
        .when(agent.flagged, |title| {
            title.child(
                div()
                    .min_w_0()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(label_color)
                    .truncate()
                    .child(agent.label.clone()),
            )
        });
    let failed = agent.state == AgentState::Failed;
    let line = div().text_color(if failed { c.destructive } else { c.muted_foreground }).child(agent_line(agent));
    let (store, item, thread) = (ctx.store.clone(), model.row.item.clone(), agent.thread.clone());
    div()
        .id(group.clone())
        .group(group.clone())
        .relative()
        .pt(px(above))
        .pb(px(below))
        .px(px(inset))
        .when(place.is_some(), |row| row.overflow_hidden())
        .children(card)
        .child(light.render(group, cx))
        .child(agent_line_box(avatar, title, line, agent_time(agent), false, cx))
        .on_click(move |_, _, cx| {
            store.update(cx, |store, cx| {
                match &thread {
                    Some(thread) => store.open_started_thread(thread, cx),
                    None => store.show_agent(item.clone(), cx),
                }
                cx.notify();
            })
        })
        .into_any_element()
}

/// Agents started one after the other: their avatars side by side, how many and how they are
/// doing, and how long they have worked. A click opens it into a card of their rows.
fn agents_row(model: &RowModel, ctx: &RowContext, cx: &App) -> AnyElement {
    let RowKind::Agents { title, summary, avatars, more, started_at, duration_ms, open } = &model.row.kind else {
        return div().into_any_element();
    };
    let c = colors(cx);
    let group: SharedString = format!("{}-agents", model.row.id).into();
    let avatars = crate::ui::AgentAvatars::new(avatars.iter().map(|avatar| avatar.agent).collect(), *more);
    let title = div()
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c.foreground)
        .child(div().min_w_0().truncate().child(title.clone()));
    let line = div().child(summary_text(summary, c));
    let time = match started_at {
        Some(started) => elapsed(*started),
        None => duration_ms.map(duration).unwrap_or_default(),
    };
    let (view, row_id) = (ctx.view.clone(), model.row.id.clone());
    div()
        .id(group.clone())
        .group(group.clone())
        .relative()
        .child(crate::ui::highlight(group, Radius::MD, edges(2., -AGENT_BLEED, 2., -AGENT_BLEED), false, cx))
        .child(agent_line_box(avatars, title, line, time, *open, cx))
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |view, cx| view.toggle_in_core(&row_id, cx));
        })
        .into_any_element()
}

/// Each count in its state's colour, between muted dots.
fn summary_text(counts: &[StateCount], c: &Colors) -> StyledText {
    let normal = font(theme::UI_FONT, FontWeight::NORMAL, false);
    let (mut text, mut runs) = (String::new(), Vec::new());
    for (index, count) in counts.iter().enumerate() {
        if index > 0 {
            text.push_str(" · ");
            runs.push(run(" · ".len(), normal.clone(), c.muted_stronger_foreground));
        }
        text.push_str(&count.text);
        runs.push(run(count.text.len(), normal.clone(), crate::ui::agent_avatar::state_color(count.state, c)));
    }
    StyledText::new(text).with_runs(runs)
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
                .bg(c.destructive_tint)
                .rounded(px(Radius::LG))
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
                        .child(icons::symbol("circle-alert", 13.).text_color(c.destructive)),
                )
                .child(text_box(13., 20.5).flex_1().min_w_0().text_color(c.destructive).child(plain(
                    &format!("{}-error", model.row.id),
                    message,
                    &[],
                    &[],
                    c.destructive,
                    row_key(&model.row.id),
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
/// a file opens what the turn changed in it in the right panel; the button opens the whole diff.
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
    let agent = ctx.agent;
    // The button's light is this far from the top and the right, so that its words end as far
    // from the edge as the title starts.
    let button_margin = 14. - RowButton::PADDING;
    div()
        .pt(px(12.))
        .pb(px(14.))
        .child(
            div()
                .bg(c.background_secondary)
                .border_1()
                .border_color(c.border)
                .rounded(px(Radius::LG))
                .pb(px(CHANGES_BOTTOM - 1.))
                .child(
                    div()
                        .h(px(CHANGES_HEAD - 1.))
                        .pl(px(13.))
                        .flex()
                        .items_center()
                        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(c.foreground).child(
                            if files == 1 { "1 changed file".to_string() } else { format!("{files} changed files") },
                        ))
                        .child(div().ml(px(10.)).child(line_counts(added, removed, cx)))
                        .child(div().flex_1())
                        .child(
                            RowButton::new(
                                SharedString::from(format!("open-diff-{row_id}")),
                                "Open diff",
                                "Show what this turn changed",
                            )
                            .radius(Radius::LG - button_margin)
                            .hover(c.background_secondary_accent)
                            .height(CHANGES_HEAD - 1.)
                            .insets(edges(button_margin - 1., 2., button_margin, button_margin - 1.))
                            .on_click(move |_, _, cx| {
                                if agent {
                                    return;
                                }
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
                .children(entries.iter().map(|entry| change_entry(entry, &item, ctx, cx))),
        )
        .into_any_element()
}

const CHANGES_HEAD: f32 = 40.;
const CHANGES_ENTRY: f32 = 28.;
const CHANGES_BOTTOM: f32 = 6.;

/// One file a turn changed, or a folder of them, lit under the pointer from where it stands in.
fn change_entry(entry: &ChangeEntry, item: &str, ctx: &RowContext, cx: &App) -> impl IntoElement {
    let c = colors(cx);
    let group: SharedString = format!("entry-{item}-{}", entry.id).into();
    let (store, view) = (ctx.store.clone(), ctx.view.clone());
    let (entry_id, path, folder, turn) = (entry.id.clone(), entry.path.clone(), entry.folder, item.to_string());
    let agent = ctx.agent;
    let symbol = if entry.folder { "folder" } else { crate::panel::state::file_symbol(&entry.path) };
    // In the card's border.
    let indent = 12. + entry.depth as f32 * 16. - 1.;
    let light_start = if entry.folder { indent } else { indent + 16. } - 6.;
    let light_inset = (CHANGES_ENTRY - ControlSize::Small.height()) / 2.;
    div()
        .id(group.clone())
        .group(group.clone())
        .relative()
        .h(px(CHANGES_ENTRY))
        .flex()
        .items_center()
        .child(
            Highlight::new()
                .radius(Radius::SM)
                .inset(edges(light_inset, light_start, light_inset, 6. - 1.))
                .surface(Surface::Secondary)
                .render(group, cx),
        )
        .child(
            div()
                .relative()
                .w_full()
                .pl(px(indent))
                .pr(px(14. - 1.))
                .flex()
                .items_center()
                .child(div().w(px(16.)).flex_none().child(if entry.folder {
                    div()
                        .w(px(12.))
                        .flex()
                        .justify_center()
                        .child(
                            icons::symbol(if entry.open { "chevron-down" } else { "chevron-right" }, 8.)
                                .text_color(c.muted_stronger_foreground),
                        )
                        .into_any_element()
                } else {
                    div().into_any_element()
                }))
                .child(
                    div().w(px(24.)).flex_none().child(
                        div()
                            .w(px(16.))
                            .flex()
                            .justify_center()
                            .child(icons::symbol(symbol, 11.).text_color(c.muted_foreground)),
                    ),
                )
                .child(
                    crate::ui::middle_truncated(entry.name.clone())
                        .flex_1()
                        .font_family(theme::MONO_FONT)
                        .text_size(px(11.5))
                        .text_color(if entry.folder { c.muted_foreground } else { c.foreground }),
                )
                .child(div().ml(px(12.)).flex_none().child(line_counts(entry.added, entry.removed, cx))),
        )
        .on_click(move |_, _, cx| {
            if folder {
                let _ = view.update(cx, |view, cx| view.toggle_folder(&entry_id, cx));
                return;
            }
            if agent {
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
/// The way back to the thread whose agent started this one: a line across, broken by a round
/// button that names it. The transcript shows it above its first row and under its last.
pub fn started_by(end: bool, ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let Some((id, title)) = ctx.store.read(cx).parent_thread().map(|thread| (thread.id.clone(), thread.title.clone()))
    else {
        return div().into_any_element();
    };
    let size = ControlSize::Small;
    let line = || div().flex_1().h(px(1.)).bg(c.border);
    let store = ctx.store.clone();
    let words = format!("Started by {title}");
    let runs = vec![
        run("Started by ".len(), font(theme::UI_FONT, FontWeight::MEDIUM, false), c.muted_foreground),
        run(title.len(), font(theme::UI_FONT, FontWeight::MEDIUM, false), c.foreground),
    ];
    let pill = div()
        .id(("started-by", usize::from(end)))
        .min_w_0()
        .h(px(size.height()))
        .pl(px(size.padding() - size.symbol_outset()))
        .pr(px(size.padding()))
        .flex()
        .items_center()
        .gap(px(size.gap()))
        .rounded_full()
        .bg(c.background_accent)
        .border_1()
        .border_color(c.border_accent)
        .hover(|pill| pill.bg(c.border_accent))
        .active(|pill| pill.bg(c.border_accent))
        .cursor_pointer()
        .child(icons::symbol("corner-left-up", size.symbol()).text_color(c.muted_foreground))
        .child(div().min_w_0().text_size(px(size.text_size())).truncate().child(StyledText::new(words).with_runs(runs)))
        .on_click(move |_, _, cx| {
            store.update(cx, |store, cx| store.open_started_thread(&id, cx));
        });
    div()
        .h(px(ControlSize::Large.height()))
        .flex()
        .items_center()
        .gap(px(ControlSize::Regular.padding()))
        .child(line())
        .child(pill)
        .child(line())
        .into_any_element()
}

pub fn working_line(activity: &crate::models::Activity, cx: &App) -> AnyElement {
    let c = colors(cx);
    let asking = activity.approvals.first().map(|waiting| waiting.ask);
    let waiting = asking.is_some();
    let words = if let Some(asking) = asking {
        match asking {
            Ask::Approval => "Waiting for your approval",
            Ask::Question => "Waiting for your answer",
            Ask::Plan => "Waiting for you to review the plan",
        }
        .to_string()
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
        .text_color(c.muted_stronger_foreground)
        .font(digits.clone())
        .child(if waiting {
            div().child(words).into_any_element()
        } else {
            let runs = vec![run(words.len(), digits.clone(), c.muted_stronger_foreground)];
            Shimmer::new("working", words, runs, c.emphasized_foreground).into_any_element()
        })
        .into_any_element()
}

/// Where the thread went on with another agent or account: a line across, broken by the model it
/// left and the one it went on with.
fn handoff_row(from: &HandoffLabel, to: &HandoffLabel, ctx: &RowContext, window: &mut Window, cx: &App) -> AnyElement {
    let c = colors(cx);
    let logo = ControlSize::Regular.symbol();
    let arrow = ControlSize::Regular.small_symbol();
    let (label_gap, arrow_gap, line_gap) =
        (ControlSize::Regular.gap(), ControlSize::Small.padding(), ControlSize::Regular.padding());
    let text = |end: &HandoffLabel| -> (SharedString, Vec<TextRun>) {
        let plain = font(theme::UI_FONT, FontWeight::NORMAL, false);
        let mut text = end.label.clone();
        let mut runs = vec![run(text.len(), plain.clone(), c.foreground)];
        if let Some(account) = &end.account {
            runs.push(run(" · ".len(), plain.clone(), c.muted_stronger_foreground));
            runs.push(run(account.len(), plain, c.foreground));
            text = format!("{text} · {account}");
        }
        (text.into(), runs)
    };
    let width = |(text, runs): &(SharedString, Vec<TextRun>), window: &mut Window| {
        f32::from(window.text_system().shape_line(text.clone(), px(12.), runs, None).width).ceil()
    };
    let (from_text, to_text) = (text(from), text(to));
    let (from_width, to_width) = (width(&from_text, window), width(&to_text, window));
    let fixed = (logo + label_gap) * 2. + arrow_gap * 2. + theme::symbol_side(arrow);
    let room = (ctx.width - line_gap * 4. - fixed).max(0.);
    let (from_width, to_width) = fitted(from_width, to_width, room);
    let end = |agent: Agent, (text, runs): (SharedString, Vec<TextRun>), width: f32| {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(label_gap))
            .child(crate::ui::logos::agent_icon(agent, logo, cx))
            .child(div().w(px(width)).text_size(px(12.)).truncate().child(StyledText::new(text).with_runs(runs)))
    };
    let rule = || div().flex_1().h(px(1.)).bg(c.border);
    div()
        .h(px(ControlSize::Large.height()))
        .flex()
        .items_center()
        .child(rule())
        .child(
            div()
                .mx(px(line_gap))
                .flex()
                .flex_none()
                .items_center()
                .child(end(from.agent, from_text, from_width))
                .child(
                    div()
                        .mx(px(arrow_gap))
                        .child(icons::symbol("arrow-right", arrow).text_color(c.muted_stronger_foreground)),
                )
                .child(end(to.agent, to_text, to_width)),
        )
        .child(rule())
        .into_any_element()
}

/// Under this inner width the two buttons of an unlinked server's thread share the row in halves.
const REMOVED_HALVES_BELOW: f32 = 420.;
const REMOVED_PADDING: f32 = 16.;

/// The end of a thread whose server was unlinked, in place of the composer: why the thread can
/// only be read, and the ways on from it.
fn removed_row(model: &RowModel, title: &str, message: &str, ctx: &RowContext, cx: &App) -> AnyElement {
    let c = colors(cx);
    let halves = ctx.width - REMOVED_PADDING * 2. < REMOVED_HALVES_BELOW;
    let (settings, elsewhere) = (ctx.store.clone(), ctx.store.clone());
    let button = |id: &'static str, title: &'static str, help: &'static str| {
        let button = RowButton::new(id, title, help).bordered().hover(c.border).rest(c.background);
        if halves { button.grows() } else { button }
    };
    let (size, line) = (13., 20.5);
    div()
        .py(px(12.))
        .child(
            div()
                .flex()
                .flex_col()
                .bg(c.background_secondary)
                .border_1()
                .border_color(c.border)
                .rounded(px(Radius::LG))
                .p(px(REMOVED_PADDING - 1.))
                .child(
                    div()
                        .h(px(18.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(icons::symbol("unlink", 13.).text_color(c.muted_foreground))
                        .child(
                            div()
                                .min_w_0()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(c.foreground)
                                .truncate()
                                .child(title.to_string()),
                        ),
                )
                .child(text_box(size, line).mt(px(6. - trim(size, line))).text_color(c.muted_foreground).child(plain(
                    &format!("{}-message", model.row.id),
                    message,
                    &[],
                    &[],
                    c.muted_foreground,
                    row_key(&model.row.id),
                    cx,
                )))
                .child(
                    div()
                        .mt(px(14.))
                        .flex()
                        .justify_end()
                        .gap(px(ControlSize::Regular.gap()))
                        .child(
                            button(
                                "open-server-settings",
                                "Open Settings",
                                "Unlinked servers are deleted with their threads in settings",
                            )
                            .on_click(move |_, _, cx| {
                                settings.update(cx, |store, cx| {
                                    store.open_settings(SettingsSection::Servers, None);
                                    cx.notify();
                                })
                            }),
                        )
                        .child(
                            button(
                                "continue-elsewhere",
                                "Continue Thread",
                                "Start a thread on another server with everything that happened here",
                            )
                            .primary()
                            .on_click(move |_, _, cx| {
                                elsewhere.update(cx, |store, cx| {
                                    store.continue_on_another_server(cx);
                                    cx.notify();
                                })
                            }),
                        ),
                ),
        )
        .into_any_element()
}

/// The widths of the two labels: as wide as they are where they fit, else the shorter one whole
/// and the other what is left, or each half.
fn fitted(from: f32, to: f32, room: f32) -> (f32, f32) {
    if from + to <= room {
        return (from, to);
    }
    let half = (room / 2.).floor();
    if from <= half {
        return (from, room - from);
    }
    if to <= half {
        return (room - to, to);
    }
    (half, half)
}

pub fn render_row(model: &RowModel, ctx: &RowContext, window: &mut Window, cx: &mut App) -> AnyElement {
    match &model.row.kind {
        RowKind::User { text, links, skills, attachments, at } => {
            let files: Vec<AttachedFile> = attachments.iter().map(AttachedFile::from).collect();
            let message =
                Message { text, links: prose::link_ranges(text, links), skills: prose::skill_ranges(text, skills) };
            user_row(model, message, &files, *at, ctx, window, cx)
        }
        RowKind::Prose { .. } => prose_row(model, ctx, cx),
        RowKind::Code { language, code, .. } => code_row(model, language, code, ctx, cx),
        RowKind::Tool { .. } | RowKind::Thinking { .. } | RowKind::Group { .. } | RowKind::Fold { .. } => {
            tool_row(model, ctx, cx)
        }
        RowKind::Agent { agent } => agent_row(model, agent, ctx, cx),
        RowKind::Agents { .. } => agents_row(model, ctx, cx),
        RowKind::Media { .. } => crate::media::media_row(model, ctx, window, cx),
        RowKind::File { .. } => crate::media::file::file_row(model, ctx, window, cx),
        RowKind::Error { message } => error_row(model, message, cx),
        RowKind::Changes { files, added, removed, entries, .. } => {
            changes_row(model, *files, *added, *removed, entries, ctx, cx)
        }
        RowKind::TurnEnd { duration_ms, stopped, folded, at, .. } => {
            turn_end_row(model, *stopped, *duration_ms, *folded, *at, ctx, cx)
        }
        RowKind::Handoff { from, to } => handoff_row(from, to, ctx, window, cx),
        RowKind::Removed { title, message, .. } => removed_row(model, title, message, ctx, cx),
        RowKind::Queued { text, skills, attachments, status } => {
            let files: Vec<AttachedFile> = attachments.iter().map(AttachedFile::from).collect();
            let message = Message { text, links: Vec::new(), skills: prose::skill_ranges(text, skills) };
            queued_row(model, message, &files, status, ctx, window, cx)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::plain_parts;

    #[test]
    fn each_line_of_a_message_keeps_the_blank_lines_after_it() {
        let text = "one\ntwo\n\nthree\n\n\nfour";
        let parts: Vec<&str> = plain_parts(text).into_iter().map(|part| &text[part]).collect();
        assert_eq!(parts, ["one", "two\n", "three\n\n", "four"]);
        assert_eq!(plain_parts("one line"), vec![0..8]);
    }
}
