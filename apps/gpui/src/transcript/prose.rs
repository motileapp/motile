//! A reply's prose as blocks ready to draw: paragraphs with their styled stretches and links,
//! code boxes, rules and tables. The core counts in UTF-16 units; here every range is in bytes of
//! the paragraph's own text, and the line separators inside code are line breaks.

use std::ops::Range;

use gpui_kit::SharedString;
use motile_core::render::markdown::{BOLD, CODE, ITALIC, Para, ParaKind, Prose, STRIKE};

#[derive(Clone, Debug, PartialEq)]
pub enum ParaStyle {
    Body,
    Heading(u8),
    /// A list item, its marker apart from its text. Without a marker it is a further paragraph
    /// of the item.
    ListItem {
        depth: u8,
        marker: Option<SharedString>,
        quote: u8,
    },
    Quote(u8),
}

/// A paragraph: its text, the stretches of it that aren't plain, and its links.
#[derive(Clone, Debug)]
pub struct TextPara {
    pub style: ParaStyle,
    pub text: SharedString,
    /// The style bits of the core (`BOLD`, `ITALIC`, `CODE`, `STRIKE`, `LINK`) by byte range.
    pub runs: Vec<(Range<usize>, u32)>,
    pub links: Vec<(Range<usize>, String)>,
    /// More room above it than its style keeps, where a list, a quote or a table ended.
    pub extra_above: f32,
}

#[derive(Clone, Debug)]
pub struct CodeBox {
    pub language: String,
    pub code: SharedString,
    /// The colours of the code by byte range, as the core's palette indices; `None` until it
    /// has been highlighted.
    pub spans: Option<Vec<(Range<usize>, u32)>>,
    pub indent: f32,
}

#[derive(Clone, Debug)]
pub struct TableCell {
    pub para: TextPara,
    pub header: bool,
    /// 0 left, 1 centre, 2 right.
    pub align: u8,
}

#[derive(Clone, Debug)]
pub enum ProseBlock {
    Para(TextPara),
    Code(CodeBox),
    Rule,
    Table { columns: usize, rows: Vec<Vec<TableCell>>, extra_above: f32 },
}

#[derive(Clone, Debug)]
pub struct PreparedProse {
    pub blocks: Vec<ProseBlock>,
    /// The text as plain text, for copying a whole reply.
    pub plain: String,
}

/// What a paragraph keeps from the one after it.
pub fn spacing_after(style: &ParaStyle) -> f32 {
    match style {
        ParaStyle::Body => BLOCK_GAP,
        ParaStyle::Heading(_) => 8.,
        ParaStyle::ListItem { .. } => 6.,
        ParaStyle::Quote(_) => 8.,
    }
}

/// What a paragraph, a list, a quote and a table keep from what follows them.
pub const BLOCK_GAP: f32 = 12.;
pub const HEADING_GAP: f32 = 14.;

/// Turns a stretch of the core's text, in UTF-16 units, into a string of its own, and says
/// where each of its units is in that string.
struct Piece {
    text: String,
    /// The byte offset in `text` of each UTF-16 unit of the stretch, and of its end.
    bytes: Vec<usize>,
}

impl Piece {
    fn new(units: &[u16], code: bool) -> Self {
        let mut text = String::new();
        let mut bytes = Vec::with_capacity(units.len() + 1);
        for result in char::decode_utf16(units.iter().copied()) {
            let character = result.unwrap_or('\u{fffd}');
            let character = if code && character == '\u{2028}' { '\n' } else { character };
            bytes.push(text.len());
            if character.len_utf16() == 2 {
                bytes.push(text.len());
            }
            text.push(character);
        }
        bytes.push(text.len());
        Self { text, bytes }
    }

    /// The bytes of a range of units of the stretch, clamped to it.
    fn range(&self, start: u32, len: u32) -> Option<Range<usize>> {
        let count = self.bytes.len() - 1;
        let start = (start as usize).min(count);
        let end = (start + len as usize).min(count);
        (end > start).then(|| self.bytes[start]..self.bytes[end])
    }
}

fn trimmed_end(piece: &mut Piece) {
    while piece.text.ends_with('\n') {
        piece.text.pop();
    }
    let len = piece.text.len();
    for byte in piece.bytes.iter_mut() {
        *byte = (*byte).min(len);
    }
}

fn block_of(kind: &ParaKind) -> String {
    match kind {
        ParaKind::ListItem { quote, .. } if *quote > 0 => "quote".into(),
        ParaKind::ListItem { .. } => "list".into(),
        ParaKind::Quote { .. } => "quote".into(),
        ParaKind::Cell { table, .. } => format!("table {table}"),
        _ => String::new(),
    }
}

/// Reads the core's prose into blocks.
pub fn prepare(prose: &Prose) -> PreparedProse {
    let units: Vec<u16> = prose.text.encode_utf16().collect();
    let mut blocks: Vec<ProseBlock> = Vec::new();
    let mut plain = String::new();
    let mut previous: Option<&Para> = None;
    for para in &prose.paras {
        let start = para.start.min(units.len() as u32);
        let end = (para.start + para.len).min(units.len() as u32);
        let code = matches!(para.kind, ParaKind::Pre { .. });
        let mut piece = Piece::new(&units[start as usize..end as usize], code);
        trimmed_end(&mut piece);
        // What follows a list, a quote or a table brings the rest of the gap.
        let ended = previous.map(|previous| block_of(&previous.kind)).unwrap_or_default();
        let extra_above = if !code && !ended.is_empty() && ended != block_of(&para.kind) {
            let above = previous
                .map(|previous| style_of(&previous.kind))
                .map(|style| spacing_after(&style))
                .unwrap_or(BLOCK_GAP);
            BLOCK_GAP - above
        } else {
            0.
        };
        previous = Some(para);
        if !plain.is_empty() {
            plain.push('\n');
        }
        plain.push_str(&piece.text);

        let runs: Vec<(Range<usize>, u32)> = prose
            .runs
            .iter()
            .filter(|run| run[0] < end && run[0] + run[1] > start)
            .filter_map(|run| {
                let from = run[0].max(start) - start;
                let to = (run[0] + run[1]).min(end) - start;
                piece.range(from, to - from).map(|range| (range, run[2]))
            })
            .collect();
        let links: Vec<(Range<usize>, String)> = prose
            .links
            .iter()
            .filter(|link| link.start < end && link.start + link.len > start)
            .filter_map(|link| {
                let from = link.start.max(start) - start;
                let to = (link.start + link.len).min(end) - start;
                piece.range(from, to - from).map(|range| (range, link.url.clone()))
            })
            .collect();

        match &para.kind {
            ParaKind::Pre { depth, language, spans, .. } => {
                let spans = spans.as_ref().map(|spans| {
                    spans
                        .chunks(3)
                        .filter(|span| span.len() == 3 && span[2] > 0)
                        .filter_map(|span| piece.range(span[0], span[1]).map(|range| (range, span[2])))
                        .collect()
                });
                blocks.push(ProseBlock::Code(CodeBox {
                    language: language.clone(),
                    code: piece.text.into(),
                    spans,
                    indent: *depth as f32 * 22.,
                }));
            }
            ParaKind::Rule => blocks.push(ProseBlock::Rule),
            ParaKind::Cell { row, column, columns, header, align, .. } => {
                let cell = TableCell {
                    para: TextPara { style: ParaStyle::Body, text: piece.text.into(), runs, links, extra_above: 0. },
                    header: *header,
                    align: *align,
                };
                let starts_table =
                    !matches!(blocks.last(), Some(ProseBlock::Table { .. })) || (*row == 0 && *column == 0);
                if starts_table {
                    blocks.push(ProseBlock::Table {
                        columns: (*columns).max(1) as usize,
                        rows: Vec::new(),
                        extra_above,
                    });
                }
                let Some(ProseBlock::Table { rows, .. }) = blocks.last_mut() else { continue };
                let row = *row as usize;
                while rows.len() <= row {
                    rows.push(Vec::new());
                }
                rows[row].push(cell);
            }
            kind => {
                let mut style = style_of(kind);
                let mut text = piece.text;
                let mut runs = runs;
                let mut links = links;
                if let ParaStyle::ListItem { marker: Some(_), depth, quote } = &style {
                    // The text starts with the marker and a tab.
                    if let Some(tab) = text.find('\t') {
                        let marker: String = text[..tab].to_string();
                        let cut = tab + 1;
                        text = text[cut..].to_string();
                        let shift =
                            |range: &Range<usize>| range.start.saturating_sub(cut)..range.end.saturating_sub(cut);
                        runs = runs
                            .iter()
                            .map(|(range, bits)| (shift(range), *bits))
                            .filter(|(range, _)| !range.is_empty())
                            .collect();
                        links = links
                            .iter()
                            .map(|(range, url)| (shift(range), url.clone()))
                            .filter(|(range, _)| !range.is_empty())
                            .collect();
                        style = ParaStyle::ListItem { depth: *depth, marker: Some(marker.into()), quote: *quote };
                    }
                }
                blocks.push(ProseBlock::Para(TextPara { style, text: text.into(), runs, links, extra_above }));
            }
        }
    }
    PreparedProse { blocks, plain }
}

fn style_of(kind: &ParaKind) -> ParaStyle {
    match kind {
        ParaKind::Heading { level } => ParaStyle::Heading(*level),
        ParaKind::ListItem { depth, marker, quote } => ParaStyle::ListItem {
            depth: (*depth).max(1),
            marker: marker.then(|| SharedString::from("")),
            quote: *quote,
        },
        ParaKind::Quote { depth } => ParaStyle::Quote(*depth),
        _ => ParaStyle::Body,
    }
}

pub fn is_bold(bits: u32) -> bool {
    bits & BOLD != 0
}

pub fn is_italic(bits: u32) -> bool {
    bits & ITALIC != 0
}

pub fn is_code(bits: u32) -> bool {
    bits & CODE != 0
}

pub fn is_strike(bits: u32) -> bool {
    bits & STRIKE != 0
}

/// Spans of the core, `[start, length, colour]` in UTF-16 units of `code`, as byte ranges.
pub fn code_spans(code: &str, spans: &[u32]) -> Vec<(Range<usize>, u32)> {
    let units: Vec<u16> = code.encode_utf16().collect();
    let piece = Piece::new(&units, false);
    spans
        .chunks(3)
        .filter(|span| span.len() == 3 && span[2] > 0)
        .filter_map(|span| piece.range(span[0], span[1]).map(|range| (range, span[2])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use motile_core::render::markdown::{Block, parse};

    fn prose(markdown: &str) -> PreparedProse {
        let Some(Block::Prose(prose)) = parse(markdown).into_iter().next() else { panic!("no prose") };
        prepare(&prose)
    }

    #[test]
    fn runs_and_links_are_bytes_of_their_paragraph() {
        let prepared = prose("Café **bold** and [a link](https://x.y)\n\nSecond `code`");
        let ProseBlock::Para(first) = &prepared.blocks[0] else { panic!("not a paragraph") };
        let (range, bits) = &first.runs[0];
        assert_eq!(&first.text[range.clone()], "bold");
        assert!(is_bold(*bits));
        assert_eq!(&first.text[first.links[0].0.clone()], "a link");
        let ProseBlock::Para(second) = &prepared.blocks[1] else { panic!("not a paragraph") };
        assert_eq!(&second.text[second.runs[0].0.clone()], "code");
    }

    #[test]
    fn list_items_keep_their_marker_apart() {
        let prepared = prose("- one\n- two");
        let ProseBlock::Para(item) = &prepared.blocks[0] else { panic!("not a paragraph") };
        assert_eq!(item.text.as_ref(), "one");
        assert!(
            matches!(&item.style, ParaStyle::ListItem { marker: Some(marker), depth: 1, .. } if marker.as_ref() == "•")
        );
    }

    #[test]
    fn a_table_is_one_block_of_rows() {
        let prepared = prose("| a | b |\n|---|---|\n| 1 | 2 |");
        let ProseBlock::Table { columns, rows, .. } = &prepared.blocks[0] else { panic!("not a table") };
        assert_eq!(*columns, 2);
        assert_eq!(rows.len(), 2);
        assert!(rows[0][0].header);
        assert_eq!(rows[1][1].para.text.as_ref(), "2");
    }
}
