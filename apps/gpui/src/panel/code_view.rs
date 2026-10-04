//! Shows the files of a diff one under the other, or one whole file. Only the lines on screen
//! are drawn, so a document of any length costs what is seen of it. The heading of the file whose
//! lines are at the top stays in view, and the line numbers stay at the left while the lines move
//! sideways.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::Change;

use super::state::{ADDED, CodeDocument, CodeFile, NOTE, REMOVED, file_symbol};
use crate::theme::{self, colors, is_dark};
use crate::transcript::rows::font;

pub const HEADING_HEIGHT: f32 = 34.;
const LINE_HEIGHT: f32 = 18.;
const FILE_GAP: f32 = 12.;
const TEXT_INSET: f32 = 12.;
/// The width of a column of the code font.
const ADVANCE: f32 = 7.6;

actions!(code_view, [CopySelection, SelectAllLines, ClearSelection]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-c", CopySelection, Some("CodeView")),
        KeyBinding::new("cmd-a", SelectAllLines, Some("CodeView")),
        KeyBinding::new("escape", ClearSelection, Some("CodeView")),
    ]);
}

/// Where a file is in the document.
#[derive(Clone, Copy, Debug)]
struct Block {
    file: usize,
    top: f32,
    rows: usize,
    heading: f32,
}

impl Block {
    fn lines_top(&self) -> f32 {
        self.top + self.heading
    }

    fn bottom(&self) -> f32 {
        self.lines_top() + self.rows as f32 * LINE_HEIGHT
    }
}

/// A place in the text: a line of a file and a byte in its shown text.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Place {
    file: usize,
    line: usize,
    offset: usize,
}

/// A line as it is drawn: tabs become spaces, so columns keep their place.
fn shown(line: &str) -> String {
    if line.contains('\t') { line.replace('\t', "    ") } else { line.to_string() }
}

/// The colours of a line, by byte range of its shown text.
fn line_runs(line: &str, spans: Option<&Vec<u32>>, base: Hsla, c: &theme::Colors) -> Vec<TextRun> {
    let text = shown(line);
    let mono = font(theme::MONO_FONT, FontWeight::NORMAL, false);
    let plain = |len: usize, color: Hsla| TextRun {
        len,
        font: mono.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let Some(spans) = spans else { return vec![plain(text.len(), base)] };
    // The spans count UTF-16 units of the line as it is, tabs and all.
    let mut map = Vec::with_capacity(line.len() + 1);
    let mut at = 0;
    for character in line.chars() {
        for _ in 0..character.len_utf16() {
            map.push(at);
        }
        at += if character == '\t' { 4 } else { character.len_utf8() };
    }
    map.push(at);
    let mut runs = Vec::new();
    let mut done = 0;
    for span in spans.chunks(3) {
        if span.len() < 3 || span[2] == 0 {
            continue;
        }
        let (start, end) = (span[0] as usize, (span[0] + span[1]) as usize);
        let (Some(&from), Some(&to)) = (map.get(start), map.get(end)) else { continue };
        if from < done || to <= from {
            continue;
        }
        if from > done {
            runs.push(plain(from - done, base));
        }
        runs.push(plain(to - from, c.syntax.get(span[2] as usize).copied().unwrap_or(base)));
        done = to;
    }
    if done < text.len() {
        runs.push(plain(text.len() - done, base));
    }
    runs
}

pub struct CodeView {
    focus: FocusHandle,
    scroll: Point<Pixels>,
    viewport: Rc<Cell<Bounds<Pixels>>>,
    selection: Option<(Place, Place)>,
    selecting: Option<Place>,
    /// The document's name and the files that are closed, as last laid out.
    shown_id: String,
    revealed: u64,
    hovered_heading: Option<usize>,
}

/// What the view shows, read from the store each time it is drawn.
pub struct CodeSnapshot<'a> {
    pub document: &'a CodeDocument,
    pub collapsed: &'a HashSet<String>,
    pub reveal: Option<&'a (String, u64)>,
}

impl CodeView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            scroll: Point::default(),
            viewport: Rc::default(),
            selection: None,
            selecting: None,
            shown_id: String::new(),
            revealed: 0,
            hovered_heading: None,
        }
    }

    fn layout(document: &CodeDocument, collapsed: &HashSet<String>) -> (Vec<Block>, f32, f32, f32) {
        let mut blocks = Vec::with_capacity(document.files.len());
        let mut y = 0.;
        let mut last_number = 1u32;
        let mut columns = 0;
        for (index, file) in document.files.iter().enumerate() {
            let closed = document.headed && collapsed.contains(&file.path);
            let heading = if document.headed { HEADING_HEIGHT } else { 0. };
            let block = Block { file: index, top: y, rows: if closed { 0 } else { file.lines.len() }, heading };
            y = block.bottom() + if document.headed { FILE_GAP } else { 0. };
            blocks.push(block);
            last_number = last_number
                .max(file.old.iter().copied().max().unwrap_or(0))
                .max(file.new.iter().copied().max().unwrap_or(0));
            columns = columns.max(file.columns);
        }
        let digits = last_number.to_string().len().max(2);
        let number_width = digits as f32 * 6.8 + 12.;
        let gutter = if document.headed { 2. } else { 1. } * number_width + 4.;
        let width = gutter + TEXT_INSET + columns as f32 * ADVANCE + 24.;
        (blocks, y + 8., width, number_width)
    }

    fn block_at(blocks: &[Block], y: f32) -> Option<Block> {
        blocks.iter().rev().find(|block| block.top <= y).copied()
    }

    fn clamp(&mut self, content: Size<Pixels>) {
        let viewport = self.viewport.get().size;
        let max_x = (content.width - viewport.width).max(px(0.));
        let max_y = (content.height - viewport.height).max(px(0.));
        self.scroll.x = self.scroll.x.clamp(px(0.), max_x);
        self.scroll.y = self.scroll.y.clamp(px(0.), max_y);
    }

    /// Follows the document: the same one keeps its place, another starts at the top, and a
    /// file asked for comes into view.
    pub fn sync(&mut self, snapshot: &CodeSnapshot) {
        if snapshot.document.id != self.shown_id {
            self.shown_id = snapshot.document.id.clone();
            self.scroll = Point::default();
            self.selection = None;
        }
        let Some((path, count)) = snapshot.reveal else { return };
        if *count == self.revealed {
            return;
        }
        self.revealed = *count;
        let (blocks, ..) = Self::layout(snapshot.document, snapshot.collapsed);
        if let Some(index) = snapshot.document.files.iter().position(|file| &file.path == path) {
            self.scroll = point(px(0.), px(blocks[index].top));
        }
    }

    fn place_at(
        &self,
        document: &CodeDocument,
        collapsed: &HashSet<String>,
        position: Point<Pixels>,
        within: Option<usize>,
        window: &mut Window,
    ) -> Option<Place> {
        let (blocks, _, _, number_width) = Self::layout(document, collapsed);
        let gutter = if document.headed { 2. } else { 1. } * number_width + 4.;
        let origin = self.viewport.get().origin;
        let y = f32::from(position.y - origin.y + self.scroll.y);
        let block = match within {
            Some(file) => blocks.get(file).copied()?,
            None => Self::block_at(&blocks, y)?,
        };
        if block.rows == 0 {
            return None;
        }
        let line = (((y - block.lines_top()) / LINE_HEIGHT).floor().max(0.) as usize).min(block.rows - 1);
        let text = shown(&document.files[block.file].lines[line]);
        let x = position.x - origin.x + self.scroll.x - px(gutter + TEXT_INSET);
        let shaped = window.text_system().shape_line(
            text.clone().into(),
            px(theme::CODE_SIZE),
            &[TextRun {
                len: text.len(),
                font: font(theme::MONO_FONT, FontWeight::NORMAL, false),
                color: black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let offset = shaped.closest_index_for_x(x.max(px(0.)));
        Some(Place { file: block.file, line, offset })
    }

    fn selected_text(&self, document: &CodeDocument) -> Option<String> {
        let (from, to) = self.selection?;
        let file = document.files.get(from.file)?;
        let mut parts = Vec::new();
        for line in from.line..=to.line.min(file.lines.len().saturating_sub(1)) {
            if file.kinds.get(line) == Some(&NOTE) && from.line != to.line {
                continue;
            }
            let text = shown(&file.lines[line]);
            let start = if line == from.line { from.offset.min(text.len()) } else { 0 };
            let end = if line == to.line { to.offset.min(text.len()) } else { text.len() };
            parts.push(text.get(start..end.max(start)).unwrap_or_default().to_string());
        }
        let text = parts.join("\n");
        (!text.is_empty()).then_some(text)
    }

    fn word_at(document: &CodeDocument, place: Place) -> Option<(Place, Place)> {
        let text = shown(document.files.get(place.file)?.lines.get(place.line)?.as_ref());
        let is_word = |character: char| character.is_alphanumeric() || character == '_';
        let offset = place.offset.min(text.len());
        let start = text[..offset]
            .char_indices()
            .rev()
            .take_while(|(_, character)| is_word(*character))
            .last()
            .map_or(offset, |(index, _)| index);
        let end = text[offset..]
            .char_indices()
            .take_while(|(_, character)| is_word(*character))
            .last()
            .map_or(offset, |(index, character)| offset + index + character.len_utf8());
        (end > start).then_some((Place { offset: start, ..place }, Place { offset: end, ..place }))
    }

    /// The view of a document, its clicks on headings handed to `on_toggle` and `on_open`.
    pub fn element(
        &mut self,
        snapshot: CodeSnapshot,
        on_toggle: impl Fn(&str, &mut Window, &mut App) + 'static,
        on_open: impl Fn(&str, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync(&snapshot);
        let document = snapshot.document.clone();
        let collapsed = snapshot.collapsed.clone();
        let (blocks, content_height, content_width, number_width) = Self::layout(&document, &collapsed);
        let content = size(px(content_width), px(content_height));
        self.clamp(content);
        let gutter = if document.headed { 2. } else { 1. } * number_width + 4.;
        let scroll = self.scroll;
        let selection = self.selection;
        let hovered_heading = self.hovered_heading;
        let viewport = self.viewport.clone();
        let c = *colors(cx);
        let dark = is_dark(cx);
        let document = Rc::new(document);
        let collapsed = Rc::new(collapsed);
        let on_toggle = Rc::new(on_toggle);
        let on_open = Rc::new(on_open);

        let paint_document = document.clone();
        let paint_blocks = blocks.clone();
        let (down_document, down_blocks, down_collapsed) = (document.clone(), blocks.clone(), collapsed.clone());
        let (move_document, move_collapsed) = (document.clone(), collapsed.clone());
        let (copy_document, all_document) = (document.clone(), document.clone());
        let canvas = canvas(
            move |bounds, _, _| {
                viewport.set(bounds);
            },
            move |bounds, _, window, cx| {
                let document = &paint_document;
                let visible_top = f32::from(scroll.y);
                let visible_bottom = visible_top + f32::from(bounds.size.height);
                let added_fill = if dark { hsla(0.35, 0.49, 0.48, 0.15) } else { hsla(0.37, 0.66, 0.30, 0.11) };
                let removed_fill = if dark { hsla(0.0, 0.92, 0.63, 0.15) } else { hsla(0.99, 0.70, 0.47, 0.09) };
                let selection_fill = if dark { hsla(0.62, 1., 0.65, 0.35) } else { hsla(0.62, 0.68, 0.50, 0.22) };
                let mono = font(theme::MONO_FONT, FontWeight::NORMAL, false);
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    for block in paint_blocks
                        .iter()
                        .filter(|block| block.bottom() + FILE_GAP >= visible_top && block.top <= visible_bottom)
                    {
                        let file = &document.files[block.file];
                        let top = bounds.origin.y + px(block.top) - scroll.y;
                        if document.headed {
                            paint_heading(
                                file,
                                block.file > 0,
                                collapsed.contains(&file.path),
                                hovered_heading == Some(block.file),
                                Bounds::new(point(bounds.origin.x, top), size(bounds.size.width, px(HEADING_HEIGHT))),
                                &c,
                                window,
                                cx,
                            );
                        }
                        if block.rows == 0 {
                            continue;
                        }
                        let first = ((visible_top - block.lines_top()) / LINE_HEIGHT).floor().max(0.) as usize;
                        let last = (((visible_bottom - block.lines_top()) / LINE_HEIGHT).floor().max(0.) as usize)
                            .min(block.rows - 1);
                        if first > last {
                            continue;
                        }
                        for line in first..=last {
                            let y = bounds.origin.y + px(block.lines_top() + line as f32 * LINE_HEIGHT) - scroll.y;
                            let row = Bounds::new(point(bounds.origin.x, y), size(bounds.size.width, px(LINE_HEIGHT)));
                            let kind = file.kinds.get(line).copied().unwrap_or(0);
                            let fill_color = match kind {
                                ADDED => Some(added_fill),
                                REMOVED => Some(removed_fill),
                                NOTE => Some(c.hover),
                                _ => None,
                            };
                            if let Some(color) = fill_color {
                                window.paint_quad(fill(row, color));
                            }
                            let text = shown(&file.lines[line]);
                            let note = kind == NOTE;
                            let runs = if note {
                                vec![TextRun {
                                    len: text.len(),
                                    font: mono.clone(),
                                    color: c.secondary,
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }]
                            } else {
                                line_runs(&file.lines[line], file.spans.get(line), c.text, &c)
                            };
                            let shaped =
                                window.text_system().shape_line(text.clone().into(), px(theme::CODE_SIZE), &runs, None);
                            let text_x = if note {
                                bounds.origin.x + px(gutter + TEXT_INSET)
                            } else {
                                bounds.origin.x + px(gutter + TEXT_INSET) - scroll.x
                            };
                            let lines_mask = Bounds::new(
                                point(bounds.origin.x + px(gutter), y),
                                size(bounds.size.width - px(gutter), px(LINE_HEIGHT)),
                            );
                            window.with_content_mask(Some(ContentMask { bounds: lines_mask }), |window| {
                                if let Some((from, to)) = selection.filter(|(from, to)| {
                                    from.file == block.file && line >= from.line && line <= to.line
                                }) {
                                    let start = if line == from.line {
                                        shaped.x_for_index(from.offset.min(text.len()))
                                    } else {
                                        px(0.)
                                    };
                                    let end = if line == to.line {
                                        shaped.x_for_index(to.offset.min(text.len()))
                                    } else {
                                        shaped.width + px(ADVANCE)
                                    };
                                    if end > start {
                                        window.paint_quad(fill(
                                            Bounds::new(point(text_x + start, y), size(end - start, px(LINE_HEIGHT))),
                                            selection_fill,
                                        ));
                                    }
                                }
                                let _ =
                                    shaped.paint(point(text_x, y), px(LINE_HEIGHT), TextAlign::Left, None, window, cx);
                            });
                            if note {
                                continue;
                            }
                            let numbers = [
                                (document.headed, file.old.get(line).copied().unwrap_or(0), number_width),
                                (true, file.new.get(line).copied().unwrap_or(0), gutter - 4.),
                            ];
                            for (shown_number, number, right) in numbers {
                                if !shown_number || number == 0 {
                                    continue;
                                }
                                let digits = number.to_string();
                                let runs = vec![TextRun {
                                    len: digits.len(),
                                    font: font(theme::MONO_FONT, FontWeight::NORMAL, false),
                                    color: c.tertiary,
                                    background_color: None,
                                    underline: None,
                                    strikethrough: None,
                                }];
                                let shaped = window.text_system().shape_line(digits.into(), px(11.), &runs, None);
                                let x = bounds.origin.x + px(right - 6.) - shaped.width;
                                let _ = shaped.paint(point(x, y), px(LINE_HEIGHT), TextAlign::Left, None, window, cx);
                            }
                        }
                    }
                    // The heading of the file whose lines are at the top stays over them.
                    if document.headed
                        && let Some(block) = paint_blocks.iter().rev().find(|block| block.top < visible_top)
                        && block.rows > 0
                        && visible_top < block.bottom()
                    {
                        let offset = (block.bottom() - HEADING_HEIGHT - visible_top).min(0.);
                        let file = &document.files[block.file];
                        let heading = Bounds::new(
                            point(bounds.origin.x, bounds.origin.y + px(offset)),
                            size(bounds.size.width, px(HEADING_HEIGHT)),
                        );
                        paint_heading(
                            file,
                            false,
                            collapsed.contains(&file.path),
                            hovered_heading == Some(block.file),
                            heading,
                            &c,
                            window,
                            cx,
                        );
                    }
                });
            },
        )
        .size_full();

        let hover_blocks = blocks;
        let hover_headed = document.headed;
        div()
            .id("code-view")
            .key_context("CodeView")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(c.background)
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                let delta = event.delta.pixel_delta(px(LINE_HEIGHT));
                this.scroll.x -= delta.x;
                this.scroll.y -= delta.y;
                this.clamp(content);
                cx.notify();
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                if hover_headed {
                    let origin = this.viewport.get().origin;
                    let y = f32::from(event.position.y - origin.y);
                    let at = y + f32::from(this.scroll.y);
                    let pinned =
                        hover_blocks.iter().rev().find(|block| block.top < f32::from(this.scroll.y)).filter(|block| {
                            block.rows > 0 && f32::from(this.scroll.y) < block.bottom() && y < HEADING_HEIGHT
                        });
                    let over = pinned.map(|block| block.file).or_else(|| {
                        Self::block_at(&hover_blocks, at).filter(|block| at < block.lines_top()).map(|block| block.file)
                    });
                    if over != this.hovered_heading {
                        this.hovered_heading = over;
                        cx.notify();
                    }
                }
                let Some(start) = this.selecting else { return };
                if event.pressed_button != Some(MouseButton::Left) {
                    this.selecting = None;
                    return;
                }
                if let Some(end) =
                    this.place_at(&move_document, &move_collapsed, event.position, Some(start.file), window)
                {
                    this.selection = Some((start.min(end), start.max(end)));
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    let origin = this.viewport.get().origin;
                    let width = f32::from(this.viewport.get().size.width);
                    let x = f32::from(event.position.x - origin.x);
                    let y = f32::from(event.position.y - origin.y) + f32::from(this.scroll.y);
                    let scrolled = f32::from(this.scroll.y);
                    // A click on a heading, the pinned one or one in its place, toggles the file or
                    // opens it.
                    if down_document.headed {
                        let pinned = down_blocks.iter().rev().find(|block| block.top < scrolled).filter(|block| {
                            block.rows > 0 && scrolled < block.bottom() && y - scrolled < HEADING_HEIGHT
                        });
                        let heading = pinned
                            .copied()
                            .or_else(|| Self::block_at(&down_blocks, y).filter(|block| y < block.lines_top()));
                        if let Some(block) = heading {
                            let path = down_document.files[block.file].path.clone();
                            if x > width - 38. {
                                on_open(&path, window, cx);
                            } else {
                                if let Some(pinned) = pinned {
                                    // A file closed from under its pinned heading leaves the view
                                    // at its heading.
                                    this.scroll.y = px(pinned.top);
                                }
                                on_toggle(&path, window, cx);
                            }
                            return;
                        }
                    }
                    let Some(start) = this.place_at(&down_document, &down_collapsed, event.position, None, window)
                    else {
                        return;
                    };
                    if event.click_count == 2 {
                        this.selection = Self::word_at(&down_document, start);
                        this.selecting = None;
                    } else {
                        this.selection = None;
                        this.selecting = Some(start);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.selecting = None))
            .on_action(cx.listener(move |this, _: &CopySelection, _, cx| {
                if let Some(text) = this.selected_text(&copy_document) {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }))
            .on_action(cx.listener(move |this, _: &SelectAllLines, _, cx| {
                let Some(file) = all_document.files.first().filter(|_| all_document.files.len() == 1) else { return };
                let last = file.lines.len().saturating_sub(1);
                let end = file.lines.last().map(|line| shown(line).len()).unwrap_or(0);
                this.selection =
                    Some((Place { file: 0, line: 0, offset: 0 }, Place { file: 0, line: last, offset: end }));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClearSelection, _, cx| {
                this.selection = None;
                cx.notify();
            }))
            .child(canvas)
            .into_any_element()
    }
}

/// The name of the file with what happened to it and how many lines changed, and the buttons
/// that close its lines and open the file.
#[allow(clippy::too_many_arguments)]
fn paint_heading(
    file: &CodeFile,
    line_above: bool,
    closed: bool,
    hovered: bool,
    bounds: Bounds<Pixels>,
    c: &theme::Colors,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(fill(bounds, c.code_background));
    if hovered {
        window.paint_quad(fill(bounds, c.hover.opacity(0.5)));
    }
    if line_above {
        window.paint_quad(fill(Bounds::new(bounds.origin, size(bounds.size.width, px(1.))), c.border));
    }
    window.paint_quad(fill(
        Bounds::new(point(bounds.origin.x, bounds.bottom() - px(1.)), size(bounds.size.width, px(1.))),
        c.border,
    ));
    let middle = bounds.origin.y + bounds.size.height / 2.;
    let icon = |name: &str, size: f32, x: Pixels, color: Hsla, window: &mut Window, cx: &mut App| {
        let side = px(size * 1.15);
        let icon_bounds = Bounds::new(point(x - side / 2., middle - side / 2.), gpui_kit::size(side, side));
        let _ =
            window.paint_svg(icon_bounds, crate::ui::icons::path(name), None, TransformationMatrix::unit(), color, cx);
    };
    icon(if closed { "chevron.right" } else { "chevron.down" }, 9., bounds.origin.x + px(16.), c.tertiary, window, cx);
    icon(file_symbol(&file.path), 11., bounds.origin.x + px(37.), c.secondary, window, cx);
    let open_x = bounds.right() - px(34.);
    icon("arrow.up.forward.square", 12., open_x + px(14.), c.secondary, window, cx);

    let mut counts_x = open_x - px(4.);
    if file.added + file.removed > 0 {
        let (text, runs) = counts_text(file.added, file.removed, c);
        let shaped = window.text_system().shape_line(text.into(), px(11.5), &runs, None);
        counts_x -= shaped.width;
        let _ = shaped.paint(point(counts_x, bounds.origin.y), bounds.size.height, TextAlign::Left, None, window, cx);
    } else {
        counts_x = open_x;
    }

    let folder = std::path::Path::new(&file.path)
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = crate::models::last_component(&file.path);
    let note = match file.change {
        Some(Change::Added) => Some("new".to_string()),
        Some(Change::Deleted) => Some("deleted".to_string()),
        Some(Change::Renamed) => file.from.as_ref().map(|from| format!("was {from}")),
        _ => None,
    };
    let mut text = String::new();
    let mut runs = Vec::new();
    let system = |weight| font(theme::SYSTEM_FONT, weight, false);
    if !folder.is_empty() {
        let part = format!("{folder}/");
        runs.push(TextRun {
            len: part.len(),
            font: system(FontWeight::NORMAL),
            color: c.secondary,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        text.push_str(&part);
    }
    runs.push(TextRun {
        len: name.len(),
        font: system(FontWeight::MEDIUM),
        color: c.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    });
    text.push_str(&name);
    if let Some(note) = note {
        let part = format!("   {note}");
        runs.push(TextRun {
            len: part.len(),
            font: system(FontWeight::NORMAL),
            color: c.tertiary,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        text.push_str(&part);
    }
    let name_x = bounds.origin.x + px(50.);
    let room = counts_x - px(10.) - name_x;
    let shaped = window.text_system().shape_line(text.into(), px(12.5), &runs, None);
    window.with_content_mask(
        Some(ContentMask {
            bounds: Bounds::new(point(name_x, bounds.origin.y), size(room.max(px(0.)), bounds.size.height)),
        }),
        |window| {
            let _ = shaped.paint(point(name_x, bounds.origin.y), bounds.size.height, TextAlign::Left, None, window, cx);
        },
    );
}

/// `+12 −3`, the lines added in green and the removed in red, leaving out a zero beside a count.
pub fn counts_text(added: u32, removed: u32, c: &theme::Colors) -> (String, Vec<TextRun>) {
    let digits = Font {
        features: FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])),
        ..font(theme::SYSTEM_FONT, FontWeight::MEDIUM, false)
    };
    let mut text = String::new();
    let mut runs = Vec::new();
    if added > 0 || removed == 0 {
        let part = format!("+{added}");
        runs.push(TextRun {
            len: part.len(),
            font: digits.clone(),
            color: c.success,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        text.push_str(&part);
    }
    if removed > 0 || added == 0 {
        let part = format!("{}−{removed}", if text.is_empty() { "" } else { " " });
        runs.push(TextRun {
            len: part.len(),
            font: digits,
            color: c.danger,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
        text.push_str(&part);
    }
    (text, runs)
}
