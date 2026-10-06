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

use super::state::{ADDED, CodeDocument, CodeFile, CodeMarks, NOTE, REMOVED, file_symbol};
use crate::theme::{self, colors, is_dark};
use crate::transcript::rows::font;

pub const HEADING_HEIGHT: f32 = 34.;
const LINE_HEIGHT: f32 = 18.;
const FILE_GAP: f32 = 12.;
const TEXT_INSET: f32 = 8.;
/// The width of a column of the code font.
const ADVANCE: f32 = 7.6;
/// The room the button that opens the file takes at the right of a heading.
const OPEN_WIDTH: f32 = 38.;
/// How wide the box that marks a file viewed is, with its word.
const VIEWED_WIDTH: f32 = 70.;

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
pub struct Place {
    pub file: usize,
    pub line: usize,
    pub offset: usize,
}

/// The line of a file a comment goes on, as GitHub counts it, and on which side.
#[derive(Clone, PartialEq, Debug)]
pub struct CommentedLine {
    pub path: String,
    pub line: u32,
    /// "left" or "right".
    pub side: &'static str,
    pub code: String,
}

/// What a click on a file's heading does.
enum HeadingPress {
    Toggle,
    Open,
    Viewed,
}

/// A line as it is drawn: tabs become spaces, so columns keep their place.
fn shown(line: &str) -> String {
    if line.contains('\t') { line.replace('\t', "    ") } else { line.to_string() }
}

fn run(len: usize, font: Font, color: Hsla) -> TextRun {
    TextRun { len, font, color, background_color: None, underline: None, strikethrough: None }
}

/// The colours of a line, by byte range of its shown text.
fn line_runs(line: &str, spans: Option<&Vec<u32>>, base: Hsla, c: &theme::Colors) -> Vec<TextRun> {
    let text = shown(line);
    let mono = font(theme::MONO_FONT, FontWeight::NORMAL, false);
    let Some(spans) = spans else { return vec![run(text.len(), mono, base)] };
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
            runs.push(run(from - done, mono.clone(), base));
        }
        runs.push(run(to - from, mono.clone(), c.syntax.get(span[2] as usize).copied().unwrap_or(base)));
        done = to;
    }
    if done < text.len() {
        runs.push(run(text.len() - done, mono, base));
    }
    runs
}

/// The line of the file at the place, as GitHub counts it, and on which side, when a comment
/// can be written on it.
fn commentable(document: &CodeDocument, marks: &CodeMarks, place: Place) -> Option<CommentedLine> {
    if !marks.commentable {
        return None;
    }
    let file = document.files.get(place.file)?;
    let code = file.lines.get(place.line)?.to_string();
    let (number, side) = match file.kind(place.line) {
        NOTE => return None,
        REMOVED => (file.old.get(place.line).copied().unwrap_or(0), "left"),
        _ => (file.new.get(place.line).copied().unwrap_or(0), "right"),
    };
    (number > 0).then(|| CommentedLine { path: file.path.clone(), line: number, side, code })
}

type OnPath = Rc<dyn Fn(&str, &mut Window, &mut App)>;
type OnLine = Rc<dyn Fn(CommentedLine, &mut Window, &mut App)>;

/// What the clicks in a document do: on a heading's chevron, its open button and its viewed box,
/// and on a line's number.
#[derive(Clone)]
pub struct CodeActions {
    pub on_toggle: OnPath,
    pub on_open: OnPath,
    pub on_viewed: OnPath,
    pub on_comment: OnLine,
}

impl Default for CodeActions {
    fn default() -> Self {
        Self {
            on_toggle: Rc::new(|_, _, _| {}),
            on_open: Rc::new(|_, _, _| {}),
            on_viewed: Rc::new(|_, _, _| {}),
            on_comment: Rc::new(|_, _, _| {}),
        }
    }
}

impl CodeActions {
    pub fn toggle(mut self, on_toggle: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Rc::new(on_toggle);
        self
    }

    pub fn open(mut self, on_open: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_open = Rc::new(on_open);
        self
    }

    pub fn viewed(mut self, on_viewed: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_viewed = Rc::new(on_viewed);
        self
    }

    pub fn comment(mut self, on_comment: impl Fn(CommentedLine, &mut Window, &mut App) + 'static) -> Self {
        self.on_comment = Rc::new(on_comment);
        self
    }
}

pub struct CodeView {
    focus: FocusHandle,
    scroll: Point<Pixels>,
    viewport: Rc<Cell<Bounds<Pixels>>>,
    selection: Option<(Place, Place)>,
    selecting: Option<Place>,
    /// The document's name, as last laid out.
    shown_id: String,
    revealed: u64,
    /// The line under the pointer, which shows that a comment can be written on it.
    hovered: Option<(usize, usize)>,
}

/// What the view shows, read from the store each time it is drawn.
pub struct CodeSnapshot<'a> {
    pub document: &'a CodeDocument,
    pub collapsed: &'a HashSet<String>,
    pub reveal: Option<&'a (String, u64)>,
    pub marks: &'a CodeMarks,
}

/// Where everything is in the document, for drawing and for clicks.
struct Layout {
    blocks: Vec<Block>,
    height: f32,
    width: f32,
    number_width: f32,
    gutter: f32,
}

impl Layout {
    fn block_at(&self, y: f32) -> Option<Block> {
        self.blocks.iter().rev().find(|block| block.top <= y).copied()
    }

    /// The file whose heading has scrolled out while its lines are at the top.
    fn pinned(&self, scrolled: f32) -> Option<Block> {
        self.blocks
            .iter()
            .rev()
            .find(|block| block.top < scrolled)
            .filter(|block| block.rows > 0 && scrolled < block.bottom())
            .copied()
    }

    fn line_at(&self, block: &Block, y: f32) -> usize {
        (((y - block.lines_top()) / LINE_HEIGHT).floor().max(0.) as usize).min(block.rows.saturating_sub(1))
    }
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
            hovered: None,
        }
    }

    fn layout(document: &CodeDocument, collapsed: &HashSet<String>) -> Layout {
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
        Layout { blocks, height: y + 8., width, number_width, gutter }
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
    fn sync(&mut self, snapshot: &CodeSnapshot, layout: &Layout) {
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
        if let Some(index) = snapshot.document.files.iter().position(|file| &file.path == path) {
            self.scroll = point(px(0.), px(layout.blocks[index].top));
        }
    }

    /// The place in the text nearest to the point, in the file `within` when one is given.
    fn place_at(
        &self,
        document: &CodeDocument,
        layout: &Layout,
        position: Point<Pixels>,
        within: Option<usize>,
        window: &mut Window,
    ) -> Option<Place> {
        let origin = self.viewport.get().origin;
        let y = f32::from(position.y - origin.y + self.scroll.y);
        let block = match within {
            Some(file) => layout.blocks.get(file).copied()?,
            None => layout.block_at(y)?,
        };
        if block.rows == 0 {
            return None;
        }
        let line = layout.line_at(&block, y);
        let text = shown(&document.files[block.file].lines[line]);
        let x = position.x - origin.x + self.scroll.x - px(layout.gutter + TEXT_INSET);
        let shaped = window.text_system().shape_line(
            text.clone().into(),
            px(theme::CODE_SIZE),
            &[run(text.len(), font(theme::MONO_FONT, FontWeight::NORMAL, false), black())],
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
            if file.kind(line) == NOTE && from.line != to.line {
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

    /// What a click in the heading of a file, as wide as `width`, `x` from its left, does.
    fn heading_press(
        x: f32,
        width: f32,
        file: &CodeFile,
        marks: &CodeMarks,
        c: &theme::Colors,
        window: &mut Window,
    ) -> HeadingPress {
        if x > width - OPEN_WIDTH {
            return HeadingPress::Open;
        }
        let Some((left, right)) = viewed_box(file, width, marks, c, window) else { return HeadingPress::Toggle };
        if left <= x && x <= right {
            return HeadingPress::Viewed;
        }
        HeadingPress::Toggle
    }

    /// The view of a document, its clicks handed to `actions`.
    pub fn element(&mut self, snapshot: CodeSnapshot, actions: CodeActions, cx: &mut Context<Self>) -> AnyElement {
        let layout = Rc::new(Self::layout(snapshot.document, snapshot.collapsed));
        self.sync(&snapshot, &layout);
        let content = size(px(layout.width), px(layout.height));
        self.clamp(content);
        let scroll = self.scroll;
        let selection = self.selection;
        let hovered = self.hovered;
        let viewport = self.viewport.clone();
        let c = *colors(cx);
        let dark = is_dark(cx);
        let document = Rc::new(snapshot.document.clone());
        let collapsed = Rc::new(snapshot.collapsed.clone());
        let marks = Rc::new(snapshot.marks.clone());

        let (paint_document, paint_layout, paint_collapsed, paint_marks) =
            (document.clone(), layout.clone(), collapsed.clone(), marks.clone());
        let (down_document, down_layout, down_marks) = (document.clone(), layout.clone(), marks.clone());
        let (move_document, move_layout, move_marks) = (document.clone(), layout.clone(), marks.clone());
        let (copy_document, all_document) = (document.clone(), document.clone());
        let canvas = canvas(
            move |bounds, _, _| {
                viewport.set(bounds);
            },
            move |bounds, _, window, cx| {
                let document = &paint_document;
                let layout = &paint_layout;
                let visible_top = f32::from(scroll.y);
                let visible_bottom = visible_top + f32::from(bounds.size.height);
                let added_fill = if dark { hsla(0.35, 0.49, 0.48, 0.15) } else { hsla(0.37, 0.66, 0.30, 0.11) };
                let removed_fill = if dark { hsla(0.0, 0.92, 0.63, 0.15) } else { hsla(0.99, 0.70, 0.47, 0.09) };
                let selection_fill = if dark { hsla(0.62, 1., 0.65, 0.35) } else { hsla(0.62, 0.68, 0.50, 0.22) };
                let mono = font(theme::MONO_FONT, FontWeight::NORMAL, false);
                let gutter = layout.gutter;
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    for block in layout
                        .blocks
                        .iter()
                        .filter(|block| block.bottom() + FILE_GAP >= visible_top && block.top <= visible_bottom)
                    {
                        let file = &document.files[block.file];
                        let top = bounds.origin.y + px(block.top) - scroll.y;
                        if document.headed {
                            paint_heading(
                                file,
                                block.file > 0,
                                paint_collapsed.contains(&file.path),
                                &paint_marks,
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
                        let marked = paint_marks.marked.get(&file.path);
                        for line in first..=last {
                            let y = bounds.origin.y + px(block.lines_top() + line as f32 * LINE_HEIGHT) - scroll.y;
                            let row = Bounds::new(point(bounds.origin.x, y), size(bounds.size.width, px(LINE_HEIGHT)));
                            let kind = file.kind(line);
                            let fill_color = match kind {
                                ADDED => Some(added_fill),
                                REMOVED => Some(removed_fill),
                                NOTE => Some(c.background_secondary),
                                _ => None,
                            };
                            if let Some(color) = fill_color {
                                window.paint_quad(fill(row, color));
                            }
                            let text = shown(&file.lines[line]);
                            let note = kind == NOTE;
                            let runs = if note {
                                vec![run(text.len(), mono.clone(), c.secondary)]
                            } else {
                                line_runs(&file.lines[line], file.spans.get(line), c.text, &c)
                            };
                            let shaped =
                                window.text_system().shape_line(text.clone().into(), px(theme::CODE_SIZE), &runs, None);
                            // A note stays where it is while the lines move sideways.
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
                                    // A line that is selected to its end is lit a little past it,
                                    // as its line break.
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
                                (document.headed, file.old.get(line).copied().unwrap_or(0), layout.number_width),
                                (true, file.new.get(line).copied().unwrap_or(0), gutter - 4.),
                            ];
                            for (shown_number, number, right) in numbers {
                                if !shown_number || number == 0 {
                                    continue;
                                }
                                let digits = number.to_string();
                                let runs = vec![run(digits.len(), mono.clone(), c.tertiary)];
                                let shaped = window.text_system().shape_line(digits.into(), px(11.), &runs, None);
                                let x = bounds.origin.x + px(right - 6.) - shaped.width;
                                let _ = shaped.paint(point(x, y), px(LINE_HEIGHT), TextAlign::Left, None, window, cx);
                            }
                            if marked.is_some_and(|lines| lines.contains(&line)) {
                                let bar = Bounds::new(
                                    point(bounds.origin.x + px(3.), y + px(5.)),
                                    size(px(3.), px(LINE_HEIGHT - 10.)),
                                );
                                window.paint_quad(fill(bar, c.link).corner_radii(Corners::all(px(1.5))));
                            }
                            if paint_marks.commentable && hovered == Some((block.file, line)) {
                                paint_comment_badge(point(bounds.origin.x + px(gutter - 2.), y), &c, window, cx);
                            }
                        }
                    }
                    // The heading of the file whose lines are at the top stays over them.
                    if document.headed
                        && let Some(block) = layout.pinned(visible_top)
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
                            paint_collapsed.contains(&file.path),
                            &paint_marks,
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

        let actions = Rc::new(actions);
        let down_actions = actions.clone();
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
                // The line under the pointer shows that it can be commented on, while the pointer
                // is on the numbers.
                if move_marks.commentable {
                    let origin = this.viewport.get().origin;
                    let x = f32::from(event.position.x - origin.x);
                    let y = f32::from(event.position.y - origin.y + this.scroll.y);
                    let hovered = move_layout
                        .block_at(y)
                        .filter(|block| x < move_layout.gutter && y >= block.lines_top())
                        .and_then(|_| this.place_at(&move_document, &move_layout, event.position, None, window))
                        .filter(|place| commentable(&move_document, &move_marks, *place).is_some())
                        .map(|place| (place.file, place.line));
                    if hovered != this.hovered {
                        this.hovered = hovered;
                        cx.notify();
                    }
                }
                let Some(start) = this.selecting else { return };
                if event.pressed_button != Some(MouseButton::Left) {
                    this.selecting = None;
                    return;
                }
                if let Some(end) = this.place_at(&move_document, &move_layout, event.position, Some(start.file), window)
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
                    // A click on a heading, the pinned one or one in its place, toggles the file,
                    // opens it or marks it viewed.
                    if down_document.headed {
                        let pinned = down_layout.pinned(scrolled).filter(|_| y - scrolled < HEADING_HEIGHT);
                        let heading = pinned.or_else(|| down_layout.block_at(y).filter(|block| y < block.lines_top()));
                        if let Some(block) = heading {
                            let file = &down_document.files[block.file];
                            match Self::heading_press(x, width, file, &down_marks, &c, window) {
                                HeadingPress::Open => (down_actions.on_open)(&file.path, window, cx),
                                HeadingPress::Viewed => (down_actions.on_viewed)(&file.path, window, cx),
                                HeadingPress::Toggle => {
                                    // A file that is closed from under its pinned heading leaves
                                    // the view at its heading.
                                    if pinned.is_some() {
                                        this.scroll.y = px(block.top);
                                    }
                                    (down_actions.on_toggle)(&file.path, window, cx);
                                }
                            }
                            return;
                        }
                    }
                    let Some(start) = this.place_at(&down_document, &down_layout, event.position, None, window) else {
                        return;
                    };
                    if down_marks.commentable
                        && x < down_layout.gutter
                        && let Some(line) = commentable(&down_document, &down_marks, start)
                    {
                        (down_actions.on_comment)(line, window, cx);
                        return;
                    }
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

fn paint_symbol(name: &str, size: f32, center: Point<Pixels>, color: Hsla, window: &mut Window, cx: &mut App) {
    let side = px(theme::symbol_side(size));
    let bounds = Bounds::new(point(center.x - side / 2., center.y - side / 2.), gpui_kit::size(side, side));
    let _ = window.paint_svg(bounds, crate::ui::icons::path(name), None, TransformationMatrix::unit(), color, cx);
}

/// The sign that a comment can be written on the line, over the end of its number.
fn paint_comment_badge(top_right: Point<Pixels>, c: &theme::Colors, window: &mut Window, cx: &mut App) {
    let side = px(16.);
    let badge = Bounds::new(point(top_right.x - side, top_right.y + (px(LINE_HEIGHT) - side) / 2.), size(side, side));
    window.paint_quad(fill(badge, c.primary).corner_radii(Corners::all(px(4.))));
    paint_symbol("plus", 10., badge.center(), white(), window, cx);
}

/// Where a file's box to mark it viewed is in its heading, from the heading's left, when it has one.
fn viewed_box(
    file: &CodeFile,
    width: f32,
    marks: &CodeMarks,
    c: &theme::Colors,
    window: &mut Window,
) -> Option<(f32, f32)> {
    if !marks.viewable {
        return None;
    }
    let counts_width =
        if file.added + file.removed > 0 { counts_width(file.added, file.removed, c, window) + 10. } else { 0. };
    let right = width - OPEN_WIDTH - counts_width;
    Some((right - VIEWED_WIDTH, right))
}

fn counts_width(added: u32, removed: u32, c: &theme::Colors, window: &mut Window) -> f32 {
    let (text, runs) = counts_text(added, removed, c);
    f32::from(window.text_system().shape_line(text.into(), px(11.5), &runs, None).width)
}

fn paint_viewed(viewed: bool, left: f32, bounds: Bounds<Pixels>, c: &theme::Colors, window: &mut Window, cx: &mut App) {
    let box_bounds =
        Bounds::new(point(bounds.origin.x + px(left + 6.), bounds.center().y - px(7.)), size(px(14.), px(14.)));
    if viewed {
        window.paint_quad(fill(box_bounds, c.primary).corner_radii(Corners::all(px(3.5))));
        paint_symbol("check", 9., box_bounds.center(), white(), window, cx);
    } else {
        window.paint_quad(
            outline(box_bounds, c.border_secondary, BorderStyle::Solid).corner_radii(Corners::all(px(3.5))),
        );
    }
    let label = "Viewed";
    let runs = vec![run(
        label.len(),
        font(theme::UI_FONT, FontWeight::NORMAL, false),
        if viewed { c.text } else { c.secondary },
    )];
    let shaped = window.text_system().shape_line(label.into(), px(11.5), &runs, None);
    let _ = shaped.paint(
        point(box_bounds.right() + px(6.), bounds.origin.y),
        bounds.size.height,
        TextAlign::Left,
        None,
        window,
        cx,
    );
}

/// The name of the file with what happened to it and how many lines changed, and the buttons
/// that close its lines and open the file. A heading under the panel's bar has the bar's line
/// above it.
#[allow(clippy::too_many_arguments)]
fn paint_heading(
    file: &CodeFile,
    line_above: bool,
    closed: bool,
    marks: &CodeMarks,
    bounds: Bounds<Pixels>,
    c: &theme::Colors,
    window: &mut Window,
    cx: &mut App,
) {
    window.paint_quad(fill(bounds, c.background_secondary));
    if line_above {
        window.paint_quad(fill(Bounds::new(bounds.origin, size(bounds.size.width, px(1.))), c.border));
    }
    window.paint_quad(fill(
        Bounds::new(point(bounds.origin.x, bounds.bottom() - px(1.)), size(bounds.size.width, px(1.))),
        c.border,
    ));
    let middle = bounds.center().y;
    paint_symbol(
        if closed { "chevron-right" } else { "chevron-down" },
        9.,
        point(bounds.origin.x + px(16.), middle),
        c.tertiary,
        window,
        cx,
    );
    paint_symbol(file_symbol(&file.path), 11., point(bounds.origin.x + px(37.), middle), c.secondary, window, cx);
    let open_x = bounds.right() - px(34.);
    paint_symbol("square-arrow-out-up-right", 12., point(open_x + px(14.), middle), c.secondary, window, cx);

    let mut counts_x = open_x - px(4.);
    if file.added + file.removed > 0 {
        let (text, runs) = counts_text(file.added, file.removed, c);
        let shaped = window.text_system().shape_line(text.into(), px(11.5), &runs, None);
        counts_x -= shaped.width;
        let _ = shaped.paint(point(counts_x, bounds.origin.y), bounds.size.height, TextAlign::Left, None, window, cx);
    } else {
        counts_x = open_x;
    }
    let viewed = viewed_box(file, f32::from(bounds.size.width), marks, c, window);
    if let Some((left, _)) = viewed {
        paint_viewed(marks.viewed.contains(&file.path), left, bounds, c, window, cx);
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
    let ui = |weight| font(theme::UI_FONT, weight, false);
    if !folder.is_empty() {
        let part = format!("{folder}/");
        runs.push(run(part.len(), ui(FontWeight::NORMAL), c.secondary));
        text.push_str(&part);
    }
    runs.push(run(name.len(), ui(FontWeight::MEDIUM), c.text));
    text.push_str(&name);
    if let Some(note) = note {
        let part = format!("   {note}");
        runs.push(run(part.len(), ui(FontWeight::NORMAL), c.tertiary));
        text.push_str(&part);
    }
    let name_x = bounds.origin.x + px(50.);
    let end = viewed.map(|(left, _)| bounds.origin.x + px(left)).unwrap_or(counts_x);
    let room = end - px(10.) - name_x;
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
        ..font(theme::UI_FONT, FontWeight::MEDIUM, false)
    };
    let mut text = String::new();
    let mut runs = Vec::new();
    if added > 0 || removed == 0 {
        let part = format!("+{added}");
        runs.push(run(part.len(), digits.clone(), c.success));
        text.push_str(&part);
    }
    if removed > 0 || added == 0 {
        let part = format!("{}−{removed}", if text.is_empty() { "" } else { " " });
        runs.push(run(part.len(), digits, c.danger));
        text.push_str(&part);
    }
    (text, runs)
}
