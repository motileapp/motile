//! Markdown to blocks an app can draw without parsing anything: stretches of styled text, and
//! code blocks. Offsets are in UTF-16 units, which is what the apps' text systems count in.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

/// A prose block is closed at the next top-level boundary once it is this long, so that no
/// single row of the transcript is expensive to lay out.
const SPLIT_AFTER: u32 = 4000;

pub const BOLD: u32 = 1;
pub const ITALIC: u32 = 2;
pub const CODE: u32 = 4;
pub const STRIKE: u32 = 8;
pub const LINK: u32 = 16;

#[derive(Clone, PartialEq, Debug)]
pub enum Block {
    Prose(Prose),
    Code { language: String, code: String },
}

#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Prose {
    pub text: String,
    /// `[start, length, style bits]` for every stretch that isn't plain.
    pub runs: Vec<[u32; 3]>,
    pub links: Vec<Link>,
    pub paras: Vec<Para>,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Link {
    pub start: u32,
    pub len: u32,
    pub url: String,
}

/// A paragraph of the text and how it is set. The range includes the paragraph's line break.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Para {
    pub start: u32,
    pub len: u32,
    #[serde(flatten)]
    pub kind: ParaKind,
}

#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParaKind {
    Body,
    Heading {
        level: u8,
    },
    /// The text starts with the item's marker and a tab; `marker` is false for the further
    /// paragraphs of an item.
    ListItem {
        depth: u8,
        marker: bool,
        quote: u8,
    },
    Quote {
        depth: u8,
    },
    /// A code block inside a list or a quote.
    Pre {
        depth: u8,
    },
    Rule,
    Cell {
        table: u16,
        row: u16,
        column: u16,
        columns: u16,
        header: bool,
        align: u8,
    },
}

struct List {
    next_number: Option<u64>,
}

struct Table {
    index: u16,
    alignments: Vec<Alignment>,
    row: u16,
    column: u16,
    header: bool,
}

struct CodeBlock {
    language: String,
    code: String,
    top_level: bool,
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    prose: Prose,
    /// The length of `prose.text` in UTF-16 units.
    length: u32,
    open_para: Option<(u32, ParaKind)>,
    lists: Vec<List>,
    quotes: u8,
    table: Option<Table>,
    tables: u16,
    code: Option<CodeBlock>,
    /// The marker waiting for the current list item's first paragraph.
    pending_marker: Option<String>,
    bold: u32,
    italic: u32,
    strike: u32,
    links: Vec<(u32, String)>,
}

pub fn parse(markdown: &str) -> Vec<Block> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut builder = Builder::default();
    for event in Parser::new_ext(markdown, options) {
        builder.event(event);
    }
    builder.finish()
}

fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

impl Builder {
    fn top_level(&self) -> bool {
        self.lists.is_empty() && self.quotes == 0 && self.table.is_none()
    }

    fn push_text(&mut self, text: &str, extra_style: u32) {
        if text.is_empty() {
            return;
        }
        let start = self.length;
        let len = utf16_len(text);
        self.prose.text.push_str(text);
        self.length += len;

        let mut style = extra_style;
        if self.bold > 0 {
            style |= BOLD;
        }
        if self.italic > 0 {
            style |= ITALIC;
        }
        if self.strike > 0 {
            style |= STRIKE;
        }
        if !self.links.is_empty() {
            style |= LINK;
        }
        if style == 0 {
            return;
        }
        match self.prose.runs.last_mut() {
            Some(run) if run[0] + run[1] == start && run[2] == style => run[1] += len,
            _ => self.prose.runs.push([start, len, style]),
        }
    }

    fn para_kind(&mut self) -> ParaKind {
        if let Some(table) = &self.table {
            let align = match table.alignments.get(table.column as usize) {
                Some(Alignment::Center) => 1,
                Some(Alignment::Right) => 2,
                _ => 0,
            };
            return ParaKind::Cell {
                table: table.index,
                row: table.row,
                column: table.column,
                columns: table.alignments.len() as u16,
                header: table.header,
                align,
            };
        }
        if !self.lists.is_empty() {
            let depth = self.lists.len() as u8;
            return ParaKind::ListItem { depth, marker: self.pending_marker.is_some(), quote: self.quotes };
        }
        if self.quotes > 0 {
            return ParaKind::Quote { depth: self.quotes };
        }
        ParaKind::Body
    }

    fn open(&mut self, kind: ParaKind) {
        self.close();
        self.open_para = Some((self.length, kind));
        if let Some(marker) = self.pending_marker.take() {
            self.push_plain(&format!("{marker}\t"));
        }
    }

    fn push_plain(&mut self, text: &str) {
        self.prose.text.push_str(text);
        self.length += utf16_len(text);
    }

    /// Opens a paragraph for inline content that arrived without one, as in a tight list.
    fn ensure_open(&mut self) {
        if self.open_para.is_some() {
            return;
        }
        let kind = self.para_kind();
        self.open(kind);
    }

    fn close(&mut self) {
        let Some((start, kind)) = self.open_para.take() else { return };
        self.push_plain("\n");
        self.prose.paras.push(Para { start, len: self.length - start, kind });
    }

    /// Ends the prose block here if it has grown long and nothing is open around it.
    fn maybe_split(&mut self) {
        if self.length >= SPLIT_AFTER && self.top_level() {
            self.flush_prose();
        }
    }

    fn flush_prose(&mut self) {
        self.close();
        if self.prose.text.is_empty() {
            return;
        }
        let mut prose = std::mem::take(&mut self.prose);
        self.length = 0;
        // The last paragraph needs no line break after it.
        if prose.text.ends_with('\n') {
            prose.text.pop();
            if let Some(last) = prose.paras.last_mut() {
                last.len -= 1;
            }
        }
        self.blocks.push(Block::Prose(prose));
    }

    fn event(&mut self, event: Event) {
        if let Some(code) = &mut self.code {
            match event {
                Event::Text(text) => code.code.push_str(&text),
                Event::End(TagEnd::CodeBlock) => self.end_code_block(),
                _ => {}
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                self.ensure_open();
                self.push_text(&text, 0);
            }
            Event::Code(text) => {
                self.ensure_open();
                self.push_text(&text, CODE);
            }
            Event::InlineMath(text) | Event::DisplayMath(text) | Event::InlineHtml(text) | Event::Html(text) => {
                self.ensure_open();
                self.push_text(&text, 0);
            }
            // A single line break in a chat message is meant as one.
            Event::SoftBreak | Event::HardBreak => {
                self.ensure_open();
                self.push_plain("\u{2028}");
            }
            Event::Rule => {
                self.open(ParaKind::Rule);
                self.push_plain("\u{a0}");
                self.close();
                self.maybe_split();
            }
            Event::TaskListMarker(checked) => {
                self.pending_marker = Some(if checked { "☑" } else { "☐" }.to_string());
            }
            Event::FootnoteReference(_) => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                let kind = self.para_kind();
                self.open(kind);
            }
            Tag::Heading { level, .. } => {
                let level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.open(ParaKind::Heading { level });
            }
            Tag::BlockQuote(_) => {
                self.close();
                self.quotes += 1;
            }
            Tag::CodeBlock(kind) => {
                self.close();
                let language = match kind {
                    CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or_default().to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                let top_level = self.top_level();
                if top_level {
                    self.flush_prose();
                }
                self.code = Some(CodeBlock { language, code: String::new(), top_level });
            }
            Tag::List(first) => {
                self.close();
                self.lists.push(List { next_number: first });
            }
            Tag::Item => {
                self.close();
                let Some(list) = self.lists.last_mut() else { return };
                let marker = match &mut list.next_number {
                    Some(number) => {
                        *number += 1;
                        format!("{}.", *number - 1)
                    }
                    None => "•".to_string(),
                };
                self.pending_marker = Some(marker);
            }
            Tag::Table(alignments) => {
                self.close();
                self.table = Some(Table { index: self.tables, alignments, row: 0, column: 0, header: false });
                self.tables += 1;
            }
            Tag::TableHead => {
                if let Some(table) = &mut self.table {
                    table.header = true;
                }
            }
            Tag::TableRow => {}
            Tag::TableCell => {
                let kind = self.para_kind();
                self.open(kind);
            }
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Strikethrough => self.strike += 1,
            Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                self.ensure_open();
                self.links.push((self.length, dest_url.to_string()));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => {
                self.close();
                self.maybe_split();
            }
            TagEnd::BlockQuote(_) => {
                self.close();
                self.quotes = self.quotes.saturating_sub(1);
                self.maybe_split();
            }
            TagEnd::List(_) => {
                self.close();
                self.lists.pop();
                self.maybe_split();
            }
            TagEnd::Item => {
                // An item with nothing in it still shows its marker.
                if self.pending_marker.is_some() {
                    self.ensure_open();
                }
                self.close();
            }
            TagEnd::TableCell => {
                self.ensure_open();
                self.close();
                if let Some(table) = &mut self.table {
                    table.column += 1;
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(table) = &mut self.table {
                    table.row += 1;
                    table.column = 0;
                    table.header = false;
                }
            }
            TagEnd::Table => {
                self.table = None;
                self.maybe_split();
            }
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Strikethrough => self.strike = self.strike.saturating_sub(1),
            TagEnd::Link | TagEnd::Image => {
                let Some((start, url)) = self.links.pop() else { return };
                if self.length > start {
                    self.prose.links.push(Link { start, len: self.length - start, url });
                }
            }
            _ => {}
        }
    }

    fn end_code_block(&mut self) {
        let Some(mut block) = self.code.take() else { return };
        if block.code.ends_with('\n') {
            block.code.pop();
        }
        if block.top_level {
            self.blocks.push(Block::Code { language: block.language, code: block.code });
            return;
        }
        let depth = self.lists.len() as u8 + self.quotes;
        self.open(ParaKind::Pre { depth });
        self.push_plain(&block.code.replace('\n', "\u{2028}"));
        self.close();
    }

    fn finish(mut self) -> Vec<Block> {
        // A code block still being streamed has no closing fence yet.
        self.end_code_block();
        self.flush_prose();
        self.blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prose(blocks: &[Block], index: usize) -> &Prose {
        match &blocks[index] {
            Block::Prose(prose) => prose,
            other => panic!("expected prose, got {other:?}"),
        }
    }

    fn utf16_slice(text: &str, start: u32, len: u32) -> String {
        let units: Vec<u16> = text.encode_utf16().collect();
        String::from_utf16(&units[start as usize..(start + len) as usize]).unwrap()
    }

    #[test]
    fn inline_styles_become_runs_over_plain_text() {
        let blocks = parse("Plain **bold** and *it* `code` ~~gone~~ [site](https://motile.app).");
        let prose = prose(&blocks, 0);

        assert_eq!(prose.text, "Plain bold and it code gone site.");
        let styled: Vec<(String, u32)> =
            prose.runs.iter().map(|run| (utf16_slice(&prose.text, run[0], run[1]), run[2])).collect();
        assert_eq!(
            styled,
            vec![
                ("bold".into(), BOLD),
                ("it".into(), ITALIC),
                ("code".into(), CODE),
                ("gone".into(), STRIKE),
                ("site".into(), LINK)
            ]
        );
        assert_eq!(prose.links, vec![Link { start: 28, len: 4, url: "https://motile.app".into() }]);
        assert_eq!(prose.paras, vec![Para { start: 0, len: 33, kind: ParaKind::Body }]);
    }

    #[test]
    fn offsets_count_utf16_units() {
        let blocks = parse("😀 **b**");
        let prose = prose(&blocks, 0);
        assert_eq!(prose.runs, vec![[3, 1, BOLD]]);
    }

    #[test]
    fn code_blocks_split_the_prose_around_them() {
        let blocks = parse("Before\n\n```rust\nfn main() {}\n```\n\nAfter");

        assert_eq!(blocks.len(), 3);
        assert_eq!(prose(&blocks, 0).text, "Before");
        assert_eq!(blocks[1], Block::Code { language: "rust".into(), code: "fn main() {}".into() });
        assert_eq!(prose(&blocks, 2).text, "After");
    }

    #[test]
    fn a_code_block_still_streaming_is_shown_as_far_as_it_got() {
        let blocks = parse("Here:\n\n```python\ndef greet(name):\n    print(na");
        assert_eq!(blocks[1], Block::Code { language: "python".into(), code: "def greet(name):\n    print(na".into() });
    }

    #[test]
    fn lists_carry_their_markers_and_depth() {
        let blocks = parse("1. one\n2. two\n   - nested\n   - [x] done\n\ntail");
        let prose = prose(&blocks, 0);

        assert_eq!(prose.text, "1.\tone\n2.\ttwo\n•\tnested\n☑\tdone\ntail");
        let kinds: Vec<&ParaKind> = prose.paras.iter().map(|para| &para.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &ParaKind::ListItem { depth: 1, marker: true, quote: 0 },
                &ParaKind::ListItem { depth: 1, marker: true, quote: 0 },
                &ParaKind::ListItem { depth: 2, marker: true, quote: 0 },
                &ParaKind::ListItem { depth: 2, marker: true, quote: 0 },
                &ParaKind::Body,
            ]
        );
    }

    #[test]
    fn headings_quotes_rules_and_nested_code_are_paragraph_kinds() {
        let blocks = parse("## Title\n\n> quoted\n\n---\n\n- item\n\n  ```sh\n  ls -la\n  ```\n");
        let prose = prose(&blocks, 0);

        let kinds: Vec<&ParaKind> = prose.paras.iter().map(|para| &para.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &ParaKind::Heading { level: 2 },
                &ParaKind::Quote { depth: 1 },
                &ParaKind::Rule,
                &ParaKind::ListItem { depth: 1, marker: true, quote: 0 },
                &ParaKind::Pre { depth: 1 },
            ]
        );
        assert!(prose.text.ends_with("ls -la"));
        assert_eq!(blocks.len(), 1, "code inside a list stays in the prose");
    }

    #[test]
    fn tables_are_cells_with_their_place_in_the_grid() {
        let blocks = parse("| A | B |\n|---|--:|\n| 1 | `2` |\n");
        let prose = prose(&blocks, 0);

        assert_eq!(prose.text, "A\nB\n1\n2");
        let cells: Vec<(u16, u16, bool, u8)> = prose
            .paras
            .iter()
            .map(|para| match para.kind {
                ParaKind::Cell { row, column, header, align, columns: 2, table: 0 } => (row, column, header, align),
                ref other => panic!("expected a cell, got {other:?}"),
            })
            .collect();
        assert_eq!(cells, vec![(0, 0, true, 0), (0, 1, true, 2), (1, 0, false, 0), (1, 1, false, 2)]);
    }

    #[test]
    fn long_prose_is_split_between_paragraphs() {
        let paragraph = "word ".repeat(300);
        let markdown = [paragraph.as_str(); 8].join("\n\n");
        let blocks = parse(&markdown);

        assert!(blocks.len() > 1);
        for block in &blocks {
            let Block::Prose(prose) = block else { panic!("only prose") };
            assert!(utf16_len(&prose.text) < 2 * SPLIT_AFTER);
            assert_eq!(prose.paras.last().map(|para| para.start + para.len), Some(utf16_len(&prose.text)));
        }
    }

    #[test]
    fn single_line_breaks_are_kept() {
        let blocks = parse("first line\nsecond line");
        assert_eq!(prose(&blocks, 0).text, "first line\u{2028}second line");
    }
}
