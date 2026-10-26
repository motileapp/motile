//! Shows the files of a diff one under the other, or one whole file. Only the lines on screen
//! are drawn, so a document of any length costs what is seen of it. The heading of the file whose
//! lines are at the top stays in view, and the line numbers stay at the left while the lines move
//! sideways.

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::base::ScrollbarHandle;
use gpui_kit::component::input::{Copy, SelectAll};
use gpui_kit::component::scroll::Scrollbar;
use gpui_kit::prelude::*;
use gpui_kit::*;
use motile_protocol::wire::Change;

use super::state::{
    ADDED, CodeDocument, CodeFile, CodeMarks, CodeMedia, Loaded, NOTE, REMOVED, file_symbol, letter_columns,
    tab_columns,
};
use crate::media::pictures::{Picture, Pictures, Shown};
use crate::theme::{self, IconSize, colors};
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
const MEDIA_INSET: f32 = 12.;
const MEDIA_MAX_HEIGHT: f32 = 320.;
/// The height of an image or a video until its picture is there.
const MEDIA_WAITING_HEIGHT: f32 = 96.;
/// The pictures of images and videos are decoded no larger than this on a side.
const MEDIA_PIXELS: u32 = 1600;
/// How far a drag that selects past the top or the bottom scrolls each frame, for each point
/// the pointer is past it.
const AUTOSCROLL: f32 = 0.5;

actions!(code_view, [ClearSelection]);

/// Copy and Select All are the Edit menu's own, so the menu bar's items reach the view too.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-c", Copy, Some("CodeView")),
        KeyBinding::new("secondary-a", SelectAll, Some("CodeView")),
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
    /// The room an image or a video takes in place of the lines.
    media: f32,
}

impl Block {
    fn lines_top(&self) -> f32 {
        self.top + self.heading
    }

    fn bottom(&self) -> f32 {
        self.lines_top() + self.rows as f32 * LINE_HEIGHT + self.media
    }

    /// Whether the point, `y` down the document, is on the image or the video the file shows.
    fn on_media(&self, y: f32) -> bool {
        self.media > 0. && y >= self.lines_top() && y < self.bottom()
    }
}

/// What an image or a video of a diff shows where it is.
#[derive(Clone)]
enum MediaShown {
    Waiting,
    Failed,
    Picture(Shown),
}

/// A row that stays where it is on screen while the document is laid out again.
struct Anchor {
    path: String,
    /// Its line, or -1 for its heading.
    line: isize,
    delta: f32,
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
#[derive(Clone, Copy, PartialEq)]
enum HeadingPress {
    Toggle,
    Open,
    Viewed,
}

/// The heading under the pointer, and the part of it that takes the press.
#[derive(Clone, Copy, PartialEq)]
struct LitHeading {
    file: usize,
    press: HeadingPress,
}

/// A line as it is drawn: a tab becomes the spaces up to the next stop, every four columns.
fn shown(line: &str) -> Cow<'_, str> {
    if !line.contains('\t') {
        return Cow::Borrowed(line);
    }
    let mut text = String::with_capacity(line.len() + 8);
    let mut column = 0;
    for character in line.chars() {
        if character == '\t' {
            let spaces = tab_columns(column);
            text.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            text.push(character);
            column += letter_columns(character);
        }
    }
    Cow::Owned(text)
}

/// The byte of the line as it is that a byte of its shown text stands for.
fn original_offset(line: &str, offset: usize) -> usize {
    let (mut at, mut column) = (0, 0);
    for (index, character) in line.char_indices() {
        if at >= offset {
            return index;
        }
        if character == '\t' {
            let spaces = tab_columns(column);
            at += spaces;
            column += spaces;
        } else {
            at += character.len_utf8();
            column += letter_columns(character);
        }
    }
    line.len()
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
    let (mut at, mut column) = (0, 0);
    for character in line.chars() {
        for _ in 0..character.len_utf16() {
            map.push(at);
        }
        if character == '\t' {
            let spaces = tab_columns(column);
            at += spaces;
            column += spaces;
        } else {
            at += character.len_utf8();
            column += letter_columns(character);
        }
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
type OnMedia = Rc<dyn Fn(&str, &CodeMedia, &mut Window, &mut App)>;

/// What the clicks in a document do: on a heading's chevron, its open button and its viewed box,
/// and on a line's number.
#[derive(Clone)]
pub struct CodeActions {
    pub on_toggle: OnPath,
    pub on_open: OnPath,
    pub on_viewed: OnPath,
    pub on_comment: OnLine,
    /// Asked for an image's or a video's file once it is to be drawn.
    pub on_media: OnMedia,
    pub on_open_media: OnMedia,
}

impl Default for CodeActions {
    fn default() -> Self {
        Self {
            on_toggle: Rc::new(|_, _, _| {}),
            on_open: Rc::new(|_, _, _| {}),
            on_viewed: Rc::new(|_, _, _| {}),
            on_comment: Rc::new(|_, _, _| {}),
            on_media: Rc::new(|_, _, _, _| {}),
            on_open_media: Rc::new(|_, _, _, _| {}),
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

    /// Fetches an image's or a video's file, and opens it in the viewer once it is clicked.
    pub fn media(
        mut self,
        load: impl Fn(&str, &CodeMedia, &mut Window, &mut App) + 'static,
        open: impl Fn(&str, &CodeMedia, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_media = Rc::new(load);
        self.on_open_media = Rc::new(open);
        self
    }
}

/// How far the view is scrolled, shared with its scrollers.
#[derive(Clone, Default)]
struct Scroll {
    /// How far the content has moved up and to the left.
    at: Rc<Cell<Point<Pixels>>>,
    viewport: Rc<Cell<Bounds<Pixels>>>,
    content: Rc<Cell<Size<Pixels>>>,
}

impl Scroll {
    fn get(&self) -> Point<Pixels> {
        self.at.get()
    }

    fn y(&self) -> f32 {
        f32::from(self.at.get().y)
    }

    /// Moves to `to`, kept within the content.
    fn set(&self, to: Point<Pixels>) {
        let (viewport, content) = (self.viewport.get().size, self.content.get());
        let max_x = (content.width - viewport.width).max(px(0.));
        let max_y = (content.height - viewport.height).max(px(0.));
        self.at.set(point(to.x.clamp(px(0.), max_x), to.y.clamp(px(0.), max_y)));
    }

    fn set_y(&self, y: f32) {
        self.set(point(self.get().x, px(y)));
    }
}

impl ScrollbarHandle for Scroll {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.viewport.get()
    }

    fn offset(&self) -> Point<Pixels> {
        -self.get()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.set(-offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        self.content.get()
    }
}

/// What the view last laid out.
struct Laid {
    document: Arc<CodeDocument>,
    collapsed: Rc<HashSet<String>>,
    marks: Rc<CodeMarks>,
    layout: Rc<Layout>,
}

pub struct CodeView {
    focus: FocusHandle,
    scroll: Scroll,
    selection: Option<(Place, Place)>,
    selecting: Option<Place>,
    shown: Option<Laid>,
    revealed: u64,
    /// The file asked to be brought into view, until a document that has it is shown.
    wanted: Option<String>,
    /// The line under the pointer, which shows that a comment can be written on it.
    hovered: Option<(usize, usize)>,
    lit: Option<LitHeading>,
    /// Where the pointer is while it selects, to scroll on past the top or the bottom.
    dragged_to: Option<Point<Pixels>>,
    autoscroll: Option<Task<()>>,
}

/// What the view shows, read from the store each time it is drawn.
pub struct CodeSnapshot<'a> {
    pub document: &'a Arc<CodeDocument>,
    pub collapsed: &'a Rc<HashSet<String>>,
    pub reveal: Option<&'a (String, u64)>,
    pub marks: &'a CodeMarks,
    /// Where the images and videos the document shows are on this device, by their blobs.
    pub media: &'a HashMap<String, Loaded<PathBuf>>,
}

/// Where everything is in the document, for drawing and for clicks.
struct Layout {
    blocks: Vec<Block>,
    height: f32,
    width: f32,
    number_width: f32,
    gutter: f32,
    /// The room each file's image or video takes, by file.
    media_heights: Vec<(usize, f32)>,
}

impl Layout {
    /// The row at `y`, to find again once the document is laid out anew.
    fn anchor(&self, document: &CodeDocument, y: f32) -> Option<Anchor> {
        let block = self.block_at(y)?;
        let line = (((y - block.lines_top()) / LINE_HEIGHT).floor() as isize).clamp(-1, block.rows as isize - 1);
        let top = if line < 0 { block.top } else { block.lines_top() + line as f32 * LINE_HEIGHT };
        Some(Anchor { path: document.files[block.file].path.clone(), line, delta: y - top })
    }

    fn offset(&self, document: &CodeDocument, anchor: &Anchor) -> f32 {
        let Some(block) =
            document.files.iter().position(|file| file.path == anchor.path).and_then(|index| self.blocks.get(index))
        else {
            return 0.;
        };
        let line = anchor.line.min(block.rows as isize - 1);
        let top = if line < 0 { block.top } else { block.lines_top() + line as f32 * LINE_HEIGHT };
        top + anchor.delta
    }

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
            scroll: Scroll::default(),
            selection: None,
            selecting: None,
            shown: None,
            revealed: 0,
            wanted: None,
            hovered: None,
            lit: None,
            dragged_to: None,
            autoscroll: None,
        }
    }

    /// What the images and videos of the files that are open show: their pictures once their
    /// files are here and decoded, which starts here.
    fn media_shown(
        document: &CodeDocument,
        collapsed: &HashSet<String>,
        files: &HashMap<String, Loaded<PathBuf>>,
        cx: &mut App,
    ) -> HashMap<usize, MediaShown> {
        let mut shown = HashMap::new();
        for (index, file) in document.files.iter().enumerate() {
            let Some(media) = &file.media else { continue };
            if document.headed && collapsed.contains(&file.path) {
                continue;
            }
            let state = match files.get(&media.blob) {
                Some(Loaded::Ready(path)) => {
                    let picture = if media.video {
                        Pictures::load_video_file(&media.key(), path, MEDIA_PIXELS, cx)
                    } else {
                        Pictures::load_file(&media.key(), path, MEDIA_PIXELS, cx)
                    };
                    match picture {
                        Picture::Ready(picture) => MediaShown::Picture(picture),
                        Picture::Failed(_) => MediaShown::Failed,
                        Picture::Loading => MediaShown::Waiting,
                    }
                }
                Some(Loaded::Failed(_)) => MediaShown::Failed,
                _ => MediaShown::Waiting,
            };
            shown.insert(index, state);
        }
        shown
    }

    /// The room each image or video takes in a view `width` wide, by file.
    fn media_heights(media: &HashMap<usize, MediaShown>, width: f32, scale: f32) -> Vec<(usize, f32)> {
        let mut heights: Vec<(usize, f32)> = media
            .iter()
            .map(|(index, shown)| {
                let height = match shown {
                    MediaShown::Picture(picture) => {
                        let room = size(width - 2. * MEDIA_INSET, MEDIA_MAX_HEIGHT);
                        fitted(picture, room, scale).height + 2. * MEDIA_INSET
                    }
                    _ => MEDIA_WAITING_HEIGHT + 2. * MEDIA_INSET,
                };
                (*index, height)
            })
            .collect();
        heights.sort_by_key(|(index, _)| *index);
        heights
    }

    /// Where every file is: only the files are gone through, as the document knows its longest
    /// line and its largest number.
    fn layout(document: &CodeDocument, collapsed: &HashSet<String>, media_heights: Vec<(usize, f32)>) -> Layout {
        let mut blocks = Vec::with_capacity(document.files.len());
        let mut y = 0.;
        for (index, file) in document.files.iter().enumerate() {
            let closed = document.headed && collapsed.contains(&file.path);
            let heading = if document.headed { HEADING_HEIGHT } else { 0. };
            let media = media_heights.iter().find(|(file, _)| *file == index).map_or(0., |(_, height)| *height);
            let rows = if closed { 0 } else { file.lines.len() };
            let block = Block { file: index, top: y, rows, heading, media };
            y = block.bottom() + if document.headed { FILE_GAP } else { 0. };
            blocks.push(block);
        }
        let digits = document.last_number.max(1).to_string().len().max(2);
        let number_width = digits as f32 * 6.8 + 12.;
        let gutter = if document.headed { 2. } else { 1. } * number_width + 4.;
        let width = gutter + TEXT_INSET + document.columns as f32 * ADVANCE + 24.;
        Layout { blocks, height: y + super::END_ROOM, width, number_width, gutter, media_heights }
    }

    /// Follows the snapshot: lays the document out again only when it, the closed files or the
    /// room of its pictures changed. Another document starts at the top and drops the selection;
    /// the same one, replaced or laid out again, keeps the row at the top where it is, and a file
    /// asked for comes into view once it is there.
    fn sync(&mut self, snapshot: &CodeSnapshot, media: &HashMap<usize, MediaShown>, scale: f32) -> Rc<Layout> {
        let width = f32::from(self.scroll.viewport.get().size.width);
        let heights = Self::media_heights(media, if width > 0. { width } else { 400. }, scale);
        let replaced = self.shown.as_ref().is_none_or(|shown| !Arc::ptr_eq(&shown.document, snapshot.document));
        if let Some(shown) = &mut self.shown
            && !replaced
            && (Rc::ptr_eq(&shown.collapsed, snapshot.collapsed) || shown.collapsed == *snapshot.collapsed)
            && shown.layout.media_heights == heights
        {
            if *shown.marks != *snapshot.marks {
                shown.marks = Rc::new(snapshot.marks.clone());
            }
            return shown.layout.clone();
        }
        let layout = Rc::new(Self::layout(snapshot.document, snapshot.collapsed, heights));
        let previous = self.shown.take();
        let same = previous.as_ref().is_some_and(|shown| shown.document.id == snapshot.document.id);
        if replaced {
            self.selection = None;
            self.selecting = None;
            self.hovered = None;
            self.lit = None;
        }
        self.shown = Some(Laid {
            document: snapshot.document.clone(),
            collapsed: snapshot.collapsed.clone(),
            marks: Rc::new(snapshot.marks.clone()),
            layout: layout.clone(),
        });
        self.scroll.content.set(size(px(layout.width), px(layout.height)));
        match previous.filter(|_| same) {
            Some(previous) => {
                let anchor = previous.layout.anchor(&previous.document, self.scroll.y());
                let y = anchor.map_or(0., |anchor| layout.offset(snapshot.document, &anchor));
                self.scroll.set_y(y);
            }
            None => self.scroll.set(Point::default()),
        }
        layout
    }

    /// Brings the file asked for to the top, now or once the document that has it is shown.
    fn reveal(&mut self, reveal: Option<&(String, u64)>, document: &CodeDocument, layout: &Layout) {
        if let Some((path, count)) = reveal
            && *count != self.revealed
        {
            self.revealed = *count;
            self.wanted = Some(path.clone());
        }
        let Some(index) =
            self.wanted.as_ref().and_then(|wanted| document.files.iter().position(|file| &file.path == wanted))
        else {
            return;
        };
        self.wanted = None;
        self.scroll.set(point(px(0.), px(layout.blocks[index].top)));
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
        let origin = self.scroll.viewport.get().origin;
        let scroll = self.scroll.get();
        let y = f32::from(position.y - origin.y + scroll.y);
        let block = match within {
            Some(file) => layout.blocks.get(file).copied()?,
            None => layout.block_at(y)?,
        };
        if block.rows == 0 {
            return None;
        }
        let y = y.min(block.bottom() - 1.);
        let line = layout.line_at(&block, y);
        let text = shown(&document.files[block.file].lines[line]).into_owned();
        let x = position.x - origin.x + scroll.x - px(layout.gutter + TEXT_INSET);
        let shaped = window.text_system().shape_line(
            text.clone().into(),
            px(theme::CODE_SIZE),
            &[run(text.len(), font(theme::MONO_FONT, FontWeight::NORMAL, false), transparent_black())],
            None,
        );
        let offset = shaped.closest_index_for_x(x.max(px(0.)));
        Some(Place { file: block.file, line, offset })
    }

    /// What is selected, as the lines are: with their tabs.
    fn selected_text(&self) -> Option<String> {
        let (from, to) = self.selection?;
        let file = self.shown.as_ref()?.document.files.get(from.file)?;
        let mut parts = Vec::new();
        for line in from.line..=to.line.min(file.lines.len().saturating_sub(1)) {
            if file.kind(line) == NOTE && from.line != to.line {
                continue;
            }
            let text: &str = &file.lines[line];
            let start = if line == from.line { original_offset(text, from.offset) } else { 0 };
            let end = if line == to.line { original_offset(text, to.offset) } else { text.len() };
            parts.push(text.get(start..end.max(start)).unwrap_or_default().to_string());
        }
        let text = parts.join("\n");
        (!text.is_empty()).then_some(text)
    }

    /// Selects all of a document that is one file.
    fn select_all(&mut self) {
        let Some(document) = self.shown.as_ref().map(|shown| shown.document.clone()) else { return };
        let Some(file) = document.files.first().filter(|_| document.files.len() == 1) else { return };
        let Some(last) = file.lines.last() else { return };
        let end = Place { file: 0, line: file.lines.len() - 1, offset: shown(last).len() };
        self.selection = Some((Place { file: 0, line: 0, offset: 0 }, end));
    }

    fn word_at(document: &CodeDocument, place: Place) -> Option<(Place, Place)> {
        let text = shown(document.files.get(place.file)?.lines.get(place.line)?.as_ref()).into_owned();
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

    /// The heading at the point in the window, the pinned one or one in its place, and the part
    /// of it there.
    fn heading_at(
        &self,
        position: Point<Pixels>,
        document: &CodeDocument,
        layout: &Layout,
        marks: &CodeMarks,
        c: &theme::Colors,
        window: &mut Window,
    ) -> Option<LitHeading> {
        let viewport = self.scroll.viewport.get();
        if !document.headed || !viewport.contains(&position) {
            return None;
        }
        let width = f32::from(viewport.size.width);
        let x = f32::from(position.x - viewport.origin.x);
        let scrolled = self.scroll.y();
        let y = f32::from(position.y - viewport.origin.y) + scrolled;
        let pinned = layout.pinned(scrolled).filter(|_| y - scrolled < HEADING_HEIGHT);
        let block = pinned.or_else(|| layout.block_at(y).filter(|block| y < block.lines_top()))?;
        let press = Self::heading_press(x, width, &document.files[block.file], marks, c, window);
        Some(LitHeading { file: block.file, press })
    }

    /// Lights the heading under the pointer, and its button.
    fn light_heading(&mut self, lit: Option<LitHeading>, cx: &mut Context<Self>) {
        if lit != self.lit {
            self.lit = lit;
            cx.notify();
        }
    }

    /// The view of a document, its clicks handed to `actions`.
    pub fn element(
        &mut self,
        snapshot: CodeSnapshot,
        actions: CodeActions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let media = Rc::new(Self::media_shown(snapshot.document, snapshot.collapsed, snapshot.media, cx));
        let layout = self.sync(&snapshot, &media, window.scale_factor());
        self.reveal(snapshot.reveal, snapshot.document, &layout);
        let Some(current) = &self.shown else { return div().into_any_element() };
        let (document, collapsed, marks) = (current.document.clone(), current.collapsed.clone(), current.marks.clone());
        self.scroll.set(self.scroll.get());
        let scroll = self.scroll.get();
        let selection = self.selection;
        let hovered = self.hovered;
        let lit = self.lit;
        let viewport = self.scroll.viewport.clone();
        let c = *colors(cx);

        let (paint_document, paint_layout, paint_collapsed, paint_marks, pictures) =
            (document.clone(), layout.clone(), collapsed.clone(), marks.clone(), media.clone());
        let (down_document, down_layout, down_marks) = (document.clone(), layout.clone(), marks.clone());
        let (move_document, move_layout, move_marks) = (document.clone(), layout.clone(), marks.clone());
        let (scroll_document, scroll_layout, scroll_marks) = (document.clone(), layout.clone(), marks.clone());
        let fitted_width = f32::from(viewport.get().size.width);
        let has_media = !layout.media_heights.is_empty();
        let paint_actions = Rc::new(actions);
        let wanted_actions = paint_actions.clone();
        // A drag that selects goes on wherever the pointer goes, in the window or out of it.
        let dragging = self.selecting.map(|start| (start, cx.entity().downgrade(), document.clone(), layout.clone()));
        let canvas = canvas(
            move |bounds, window, _| {
                viewport.set(bounds);
                // The images and videos fit the view's width, as it is now.
                if has_media && (fitted_width - f32::from(bounds.size.width)).abs() >= 0.5 {
                    window.refresh();
                }
            },
            move |bounds, _, window, cx| {
                let document = &paint_document;
                let layout = &paint_layout;
                let visible_top = f32::from(scroll.y);
                let visible_bottom = visible_top + f32::from(bounds.size.height);
                let added_fill = c.success_tint;
                let removed_fill = c.destructive_tint;
                let selection_fill = c.primary_tint_stronger;
                let mono = font(theme::MONO_FONT, FontWeight::NORMAL, false);
                let digits = Font {
                    features: crate::ui::tabular_numbers(),
                    ..font(theme::SYSTEM_FONT, FontWeight::NORMAL, false)
                };
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
                                lit.filter(|lit| lit.file == block.file).map(|lit| lit.press),
                                paint_collapsed.contains(&file.path),
                                &paint_marks,
                                Bounds::new(point(bounds.origin.x, top), size(bounds.size.width, px(HEADING_HEIGHT))),
                                &c,
                                window,
                                cx,
                            );
                        }
                        if let (Some(shown), Some(media)) = (pictures.get(&block.file), &file.media)
                            && block.media > 0.
                        {
                            let room = Bounds::new(
                                point(bounds.origin.x, top + px(block.heading)),
                                size(bounds.size.width, px(block.media)),
                            )
                            .inset(px(MEDIA_INSET));
                            if matches!(shown, MediaShown::Waiting) {
                                let (load, path, media) = (wanted_actions.clone(), file.path.clone(), media.clone());
                                window.defer(cx, move |window, cx| (load.on_media)(&path, &media, window, cx));
                            }
                            paint_media(shown, media.video, room, window.scale_factor(), &c, window, cx);
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
                        let spans = file.spans();
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
                            let text = shown(&file.lines[line]).into_owned();
                            let note = kind == NOTE;
                            let runs = if note {
                                vec![run(text.len(), mono.clone(), c.muted_foreground)]
                            } else {
                                line_runs(&file.lines[line], spans.get(line), c.foreground, &c)
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
                                let number = number.to_string();
                                let runs = vec![run(number.len(), digits.clone(), c.muted_stronger_foreground)];
                                let shaped = window.text_system().shape_line(number.into(), px(11.), &runs, None);
                                let x = bounds.origin.x + px(right - 6.) - shaped.width;
                                let _ = shaped.paint(point(x, y), px(LINE_HEIGHT), TextAlign::Left, None, window, cx);
                            }
                            if marked.is_some_and(|lines| lines.contains(&line)) {
                                let bar = Bounds::new(
                                    point(bounds.origin.x + px(3.), y + px(5.)),
                                    size(px(3.), px(LINE_HEIGHT - 10.)),
                                );
                                window.paint_quad(fill(bar, c.primary).corner_radii(Corners::all(px(1.5))));
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
                            lit.filter(|lit| lit.file == block.file).map(|lit| lit.press),
                            paint_collapsed.contains(&file.path),
                            &paint_marks,
                            heading,
                            &c,
                            window,
                            cx,
                        );
                    }
                });
                let Some((start, view, document, layout)) = dragging else { return };
                let moved = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let _ = moved.update(cx, |this, cx| this.drag_to(event, start, &document, &layout, window, cx));
                });
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let _ = view.update(cx, |this, _| this.end_drag());
                });
            },
        )
        .size_full();

        let down_actions = paint_actions;
        let pointing = self.hovered.is_some();
        let selected = self.selection.is_some();
        div()
            .id("code-view")
            .key_context("CodeView")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(c.background)
            .when(pointing, |view| view.cursor_pointer())
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, window, cx| {
                let delta = event.delta.pixel_delta(px(LINE_HEIGHT));
                this.scroll.set(this.scroll.get() - delta);
                let lit = this.heading_at(event.position, &scroll_document, &scroll_layout, &scroll_marks, &c, window);
                this.light_heading(lit, cx);
                cx.notify();
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if *hovered {
                    return;
                }
                this.light_heading(None, cx);
                if this.hovered.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                let lit = this.heading_at(event.position, &move_document, &move_layout, &move_marks, &c, window);
                this.light_heading(lit, cx);
                // The line under the pointer shows that it can be commented on, while the pointer
                // is on the numbers.
                if !move_marks.commentable {
                    return;
                }
                let origin = this.scroll.viewport.get().origin;
                let x = f32::from(event.position.x - origin.x);
                let y = f32::from(event.position.y - origin.y) + this.scroll.y();
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
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    let viewport = this.scroll.viewport.get();
                    let width = f32::from(viewport.size.width);
                    let x = f32::from(event.position.x - viewport.origin.x);
                    let scrolled = this.scroll.y();
                    let y = f32::from(event.position.y - viewport.origin.y) + scrolled;
                    // A click on a heading, the pinned one or one in its place, toggles the file,
                    // opens it or marks it viewed.
                    if down_document.headed {
                        let pinned = down_layout.pinned(scrolled).filter(|_| y - scrolled < HEADING_HEIGHT);
                        let heading = pinned.or_else(|| down_layout.block_at(y).filter(|block| y < block.lines_top()));
                        if let Some(block) = down_layout.block_at(y).filter(|block| block.on_media(y))
                            && heading.is_none()
                            && let Some(media) = &down_document.files[block.file].media
                        {
                            (down_actions.on_open_media)(&down_document.files[block.file].path, media, window, cx);
                            return;
                        }
                        if let Some(block) = heading {
                            let file = &down_document.files[block.file];
                            match Self::heading_press(x, width, file, &down_marks, &c, window) {
                                HeadingPress::Open => (down_actions.on_open)(&file.path, window, cx),
                                HeadingPress::Viewed => (down_actions.on_viewed)(&file.path, window, cx),
                                HeadingPress::Toggle => {
                                    // A file that is closed from under its pinned heading leaves
                                    // the view at its heading.
                                    if pinned.is_some() {
                                        this.scroll.set_y(block.top);
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
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.end_drag()))
            .when(selected, |view| {
                view.on_action(cx.listener(|this, _: &Copy, _, cx| {
                    if let Some(text) = this.selected_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                }))
            })
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.select_all();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClearSelection, _, cx| {
                if this.selection.take().is_none() {
                    return cx.propagate();
                }
                cx.notify();
            }))
            .child(canvas)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(Scrollbar::new(&self.scroll).id("code-scroller").viewport_from_layout()),
            )
            .into_any_element()
    }
}

impl CodeView {
    /// The selection reaches to where the pointer is, in the file it began in.
    fn drag_to(
        &mut self,
        event: &MouseMoveEvent,
        start: Place,
        document: &Arc<CodeDocument>,
        layout: &Rc<Layout>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selecting.is_none() || event.pressed_button != Some(MouseButton::Left) {
            return self.end_drag();
        }
        self.dragged_to = Some(event.position);
        if let Some(end) = self.place_at(document, layout, event.position, Some(start.file), window) {
            self.selection = Some((start.min(end), start.max(end)));
            cx.notify();
        }
        self.keep_scrolling(document.clone(), layout.clone(), start, cx);
    }

    fn end_drag(&mut self) {
        self.selecting = None;
        self.dragged_to = None;
        self.autoscroll = None;
    }

    /// While a drag that selects is past the top or the bottom of the view, the view scrolls on
    /// that way and the selection follows.
    fn keep_scrolling(
        &mut self,
        document: Arc<CodeDocument>,
        layout: Rc<Layout>,
        start: Place,
        cx: &mut Context<Self>,
    ) {
        if self.autoscroll.is_some() {
            return;
        }
        self.autoscroll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let going = this.update(cx, |this, cx| {
                    let Some(pointer) = this.dragged_to.filter(|_| this.selecting.is_some()) else { return false };
                    let viewport = this.scroll.viewport.get();
                    let past = if pointer.y < viewport.top() {
                        pointer.y - viewport.top()
                    } else if pointer.y > viewport.bottom() {
                        pointer.y - viewport.bottom()
                    } else {
                        return true;
                    };
                    this.scroll.set_y(this.scroll.y() + f32::from(past) * AUTOSCROLL);
                    let inside = point(pointer.x, pointer.y.clamp(viewport.top(), viewport.bottom() - px(1.)));
                    let y = f32::from(inside.y - viewport.origin.y) + this.scroll.y();
                    if let Some(block) = layout.blocks.get(start.file).filter(|block| block.rows > 0) {
                        let line = layout.line_at(block, y);
                        let offset = if pointer.y < viewport.top() {
                            0
                        } else {
                            shown(&document.files[start.file].lines[line]).len()
                        };
                        let end = Place { file: start.file, line, offset };
                        this.selection = Some((start.min(end), start.max(end)));
                    }
                    cx.notify();
                    true
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
            let _ = this.update(cx, |this, _| this.autoscroll = None);
        }));
    }
}

/// The size a picture is drawn at: its own, or smaller to fit the room.
fn fitted(picture: &Shown, room: Size<f32>, scale: f32) -> Size<f32> {
    let (width, height) = (picture.width as f32 / scale, picture.height as f32 / scale);
    if width <= 0. || height <= 0. {
        return size(0., 0.);
    }
    let fit = (room.width / width).min(room.height / height).clamp(0., 1.);
    size((width * fit).round(), (height * fit).round())
}

/// The picture of an image or a video, as large as it is or smaller to fit, or the room it waits
/// in. A video's picture is its first frame, under the sign that it plays.
fn paint_media(
    shown: &MediaShown,
    video: bool,
    room: Bounds<Pixels>,
    scale: f32,
    c: &theme::Colors,
    window: &mut Window,
    cx: &mut App,
) {
    let MediaShown::Picture(picture) = shown else {
        window.paint_quad(fill(room, c.background_secondary).corner_radii(Corners::all(px(theme::Radius::MD))));
        if matches!(shown, MediaShown::Failed) {
            let note = if video { "This video couldn't be shown." } else { "This image couldn't be shown." };
            let runs = vec![run(note.len(), font(theme::UI_FONT, FontWeight::NORMAL, false), c.muted_foreground)];
            let shaped = window.text_system().shape_line(note.into(), px(12.5), &runs, None);
            let at = point(room.center().x - shaped.width / 2., room.origin.y);
            let _ = shaped.paint(at, room.size.height, TextAlign::Left, None, window, cx);
        }
        return;
    };
    let fit = fitted(picture, size(f32::from(room.size.width), f32::from(room.size.height)), scale);
    let frame = Bounds::new(room.origin, size(px(fit.width), px(fit.height)));
    let corners = Corners::all(px(theme::Radius::SM));
    let _ = window.paint_image(frame, frame, corners, picture.image.clone(), 0, false);
    if video {
        paint_symbol("circle-play", IconSize::Hero, frame.center(), c.foreground_over_picture, window, cx);
    }
}

fn paint_symbol(name: &str, size: IconSize, center: Point<Pixels>, color: Hsla, window: &mut Window, cx: &mut App) {
    let side = px(size.side());
    let bounds = Bounds::new(point(center.x - side / 2., center.y - side / 2.), gpui_kit::size(side, side));
    let _ = window.paint_svg(bounds, crate::ui::icons::path(name), None, TransformationMatrix::unit(), color, cx);
}

/// The sign that a comment can be written on the line, over the end of its number.
fn paint_comment_badge(top_right: Point<Pixels>, c: &theme::Colors, window: &mut Window, cx: &mut App) {
    let side = px(16.);
    let badge = Bounds::new(point(top_right.x - side, top_right.y + (px(LINE_HEIGHT) - side) / 2.), size(side, side));
    window.paint_quad(fill(badge, c.primary).corner_radii(Corners::all(px(theme::Radius::XS))));
    paint_symbol("plus", IconSize::Xs, badge.center(), c.primary_foreground, window, cx);
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
        window.paint_quad(fill(box_bounds, c.primary).corner_radii(Corners::all(px(theme::Radius::XS))));
        paint_symbol("check", IconSize::Xs, box_bounds.center(), c.primary_foreground, window, cx);
    } else {
        window.paint_quad(
            outline(box_bounds, c.border, BorderStyle::Solid).corner_radii(Corners::all(px(theme::Radius::XS))),
        );
    }
    let label = "Viewed";
    let runs = vec![run(
        label.len(),
        font(theme::UI_FONT, FontWeight::NORMAL, false),
        if viewed { c.foreground } else { c.muted_foreground },
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
    lit: Option<HeadingPress>,
    closed: bool,
    marks: &CodeMarks,
    bounds: Bounds<Pixels>,
    c: &theme::Colors,
    window: &mut Window,
    cx: &mut App,
) {
    let surface = theme::Surface::Secondary;
    let fill_color = if lit.is_some() { surface.layer(theme::Layer::Row, c) } else { c.background_secondary };
    window.paint_quad(fill(bounds, fill_color));
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
        IconSize::Xs,
        point(bounds.origin.x + px(16.), middle),
        c.muted_stronger_foreground,
        window,
        cx,
    );
    paint_symbol(
        file_symbol(&file.path),
        IconSize::Sm,
        point(bounds.origin.x + px(37.), middle),
        c.muted_foreground,
        window,
        cx,
    );
    let open_x = bounds.right() - px(34.);
    let open_center = point(open_x + px(14.), middle);
    let open_lit = lit == Some(HeadingPress::Open);
    if open_lit {
        let side = px(theme::ControlSize::Small.height());
        let button = Bounds::centered_at(open_center, size(side, side));
        let radius = Corners::all(px(theme::ControlSize::Small.radius()));
        window.paint_quad(fill(button, surface.layer(theme::Layer::ControlLit, c)).corner_radii(radius));
    }
    let open_color = if open_lit { c.foreground } else { c.muted_foreground };
    paint_symbol("square-arrow-out-up-right", theme::ControlSize::Small.symbol(), open_center, open_color, window, cx);

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
        runs.push(run(part.len(), ui(FontWeight::NORMAL), c.muted_foreground));
        text.push_str(&part);
    }
    runs.push(run(name.len(), ui(FontWeight::MEDIUM), c.foreground));
    text.push_str(&name);
    // The note is a size smaller than the name, on the same baseline.
    let note = note.map(|note| {
        let note = format!("   {note}");
        let runs = vec![run(note.len(), ui(FontWeight::NORMAL), c.muted_stronger_foreground)];
        window.text_system().shape_line(note.into(), px(11.5), &runs, None)
    });
    let note_width = note.as_ref().map_or(px(0.), |note| note.width);
    let name_x = bounds.origin.x + px(50.);
    let end = viewed.map(|(left, _)| bounds.origin.x + px(left)).unwrap_or(counts_x);
    let room = (end - px(10.) - name_x).max(px(0.));
    let (text, runs) = crate::ui::elide_middle_runs(text.into(), runs, px(12.5), (room - note_width).max(px(0.)), cx);
    let shaped = window.text_system().shape_line(text, px(12.5), &runs, None);
    let baseline = |line: &ShapedLine| (bounds.size.height - line.ascent - line.descent) / 2. + line.ascent;
    let name_baseline = baseline(&shaped);
    window.with_content_mask(
        Some(ContentMask { bounds: Bounds::new(point(name_x, bounds.origin.y), size(room, bounds.size.height)) }),
        |window| {
            let _ = shaped.paint(point(name_x, bounds.origin.y), bounds.size.height, TextAlign::Left, None, window, cx);
            if let Some(note) = note {
                let at = point(name_x + shaped.width, bounds.origin.y + name_baseline - baseline(&note));
                let _ = note.paint(at, bounds.size.height, TextAlign::Left, None, window, cx);
            }
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
        runs.push(run(part.len(), digits, c.destructive));
        text.push_str(&part);
    }
    (text, runs)
}

#[cfg(test)]
mod tests {
    use super::{original_offset, shown};

    #[test]
    fn a_tab_reaches_the_next_stop_of_four_columns() {
        assert_eq!(shown("\tx"), "    x");
        assert_eq!(shown("ab\tx"), "ab  x");
        assert_eq!(shown("abcd\tx"), "abcd    x");
        assert_eq!(shown("a\tb\tc"), "a   b   c");
    }

    #[test]
    fn a_selection_copies_the_tabs_it_was_drawn_as_spaces() {
        let line = "ab\tx";
        assert_eq!(original_offset(line, 0), 0);
        assert_eq!(original_offset(line, 2), 2);
        assert_eq!(original_offset(line, 4), 3);
        assert_eq!(original_offset(line, 5), 4);
    }
}
