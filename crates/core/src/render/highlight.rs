//! Syntax highlighting. A span is `[start, length, colour]` in UTF-16 units, where the colour is
//! an index into the palette the clients share (`Colour`), so one result serves light and dark.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex};

use syntect::highlighting::{
    Color, HighlightState, Highlighter, RangedHighlightIterator, ScopeSelectors, StyleModifier, Theme, ThemeItem,
    ThemeSettings,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

pub type Spans = Arc<Vec<u32>>;

/// Lines longer than this are left plain; they are minified data, and colouring them is slow.
const MAX_LINE_BYTES: usize = 4000;
const CACHE_ENTRIES: usize = 400;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Colour {
    Comment = 1,
    Keyword = 2,
    String = 3,
    Constant = 4,
    Function = 5,
    Type = 6,
    Variable = 7,
    Tag = 8,
    Attribute = 9,
    Inserted = 10,
    Deleted = 11,
    Heading = 12,
    Escape = 13,
    Property = 14,
    Meta = 15,
}

const SCOPES: &[(&str, Colour)] = &[
    ("comment, punctuation.definition.comment", Colour::Comment),
    ("keyword, storage, keyword.operator.word", Colour::Keyword),
    ("string, punctuation.definition.string", Colour::String),
    ("constant.numeric, constant.language, constant.character, constant.other", Colour::Constant),
    ("entity.name.function, support.function, variable.function", Colour::Function),
    (
        "entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, entity.name.namespace, entity.other.inherited-class, support.type, support.class, storage.type.primitive",
        Colour::Type,
    ),
    ("variable.parameter, variable.language", Colour::Variable),
    ("entity.name.tag", Colour::Tag),
    ("entity.other.attribute-name", Colour::Attribute),
    ("markup.inserted", Colour::Inserted),
    ("markup.deleted", Colour::Deleted),
    ("markup.heading, entity.name.section", Colour::Heading),
    ("constant.character.escape, string.regexp", Colour::Escape),
    ("meta.mapping.key string, support.type.property-name, meta.object-literal.key", Colour::Property),
    (
        "meta.diff.header, meta.diff.range, punctuation.definition.range.diff, meta.preprocessor, meta.annotation",
        Colour::Meta,
    ),
];

static SYNTAXES: LazyLock<SyntaxSet> = LazyLock::new(two_face::syntax::extra_newlines);

/// A theme whose "colours" are palette indices, carried in the red channel.
static THEME: LazyLock<Theme> = LazyLock::new(|| {
    let item = |(selectors, colour): &(&str, Colour)| ThemeItem {
        scope: ScopeSelectors::from_str(selectors).expect("valid scope selectors"),
        style: StyleModifier {
            foreground: Some(Color { r: *colour as u8, g: 0, b: 0, a: 255 }),
            background: None,
            font_style: None,
        },
    };
    Theme {
        name: None,
        author: None,
        settings: ThemeSettings { foreground: Some(Color { r: 0, g: 0, b: 0, a: 255 }), ..ThemeSettings::default() },
        scopes: SCOPES.iter().map(item).collect(),
    }
});

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

#[derive(Default)]
struct Cache {
    entries: HashMap<(String, u64), (Spans, u64)>,
    clock: u64,
}

fn syntax_for(language: &str) -> Option<&'static SyntaxReference> {
    let language = language.trim().to_lowercase();
    let token = match language.as_str() {
        "" | "text" | "plaintext" | "plain" | "txt" | "output" => return None,
        "shell" | "zsh" | "console" | "shellscript" | "shell-session" => "bash",
        "golang" => "go",
        "c++" => "cpp",
        "jsonc" | "json5" => "json",
        "typescriptreact" => "tsx",
        "javascriptreact" => "jsx",
        "objective-c" => "objc",
        "docker" => "dockerfile",
        "patch" => "diff",
        other => other,
    };
    SYNTAXES.find_syntax_by_token(token)
}

fn hash(text: &str) -> u64 {
    // FNV-1a: fast enough to run over every code block, and only used as a cache key.
    text.bytes().fold(0xcbf29ce484222325, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100000001b3))
}

/// The spans for the code if it has been highlighted before.
pub fn cached(language: &str, code: &str) -> Option<Spans> {
    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.clock += 1;
    let clock = cache.clock;
    let entry = cache.entries.get_mut(&(language.to_string(), hash(code)))?;
    entry.1 = clock;
    Some(entry.0.clone())
}

fn remember(language: &str, code: &str, spans: &Spans) {
    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.clock += 1;
    let clock = cache.clock;
    if cache.entries.len() >= CACHE_ENTRIES {
        let oldest = cache.entries.iter().min_by_key(|(_, (_, used))| *used).map(|(key, _)| key.clone());
        if let Some(oldest) = oldest {
            cache.entries.remove(&oldest);
        }
    }
    cache.entries.insert((language.to_string(), hash(code)), (spans.clone(), clock));
}

/// Highlights a whole block. Slow for long code; call it off the thread that draws.
pub fn highlight(language: &str, code: &str) -> Spans {
    if let Some(spans) = cached(language, code) {
        return spans;
    }
    let mut state = Incremental::new(language);
    let spans = state.advance(code);
    remember(language, code, &spans);
    spans
}

/// Highlights code that grows as it streams, without starting over each time.
pub struct Incremental {
    language: String,
    lines: Option<Lines>,
    /// The complete lines highlighted so far.
    done: String,
    done_utf16: u32,
    spans: Vec<u32>,
}

#[derive(Clone)]
struct Lines {
    parse: ParseState,
    highlight: HighlightState,
}

impl Incremental {
    pub fn new(language: &str) -> Self {
        let lines = syntax_for(language).map(|syntax| Lines {
            parse: ParseState::new(syntax),
            highlight: HighlightState::new(&Highlighter::new(&THEME), ScopeStack::new()),
        });
        Self { language: language.to_string(), lines, done: String::new(), done_utf16: 0, spans: Vec::new() }
    }

    /// The spans for `code`, which is usually what it was last time plus some more.
    pub fn advance(&mut self, code: &str) -> Spans {
        if self.lines.is_none() {
            return Spans::default();
        }
        if !code.starts_with(self.done.as_str()) {
            *self = Self::new(&self.language);
        }
        let rest = &code[self.done.len()..];
        let complete = rest.rfind('\n').map_or(0, |index| index + 1);
        let (whole_lines, partial) = rest.split_at(complete);

        let highlighter = Highlighter::new(&THEME);
        let Some(lines) = &mut self.lines else { return Spans::default() };
        for line in whole_lines.split_inclusive('\n') {
            highlight_line(lines, &highlighter, line, self.done_utf16, &mut self.spans);
            self.done_utf16 += line.encode_utf16().count() as u32;
        }
        self.done.push_str(whole_lines);

        let mut spans = self.spans.clone();
        if !partial.is_empty() {
            // The last line isn't finished, so it is highlighted on a copy of the state.
            let mut ahead = lines.clone();
            highlight_line(&mut ahead, &highlighter, partial, self.done_utf16, &mut spans);
        }
        let spans = Arc::new(spans);
        if partial.is_empty() {
            remember(&self.language, code, &spans);
        }
        spans
    }
}

/// Highlights lines one after the other, each with spans that count from its own start.
pub struct ByLine {
    lines: Option<Lines>,
    highlighter: Highlighter<'static>,
}

impl ByLine {
    pub fn new(language: &str) -> Self {
        let highlighter = Highlighter::new(&THEME);
        let lines = syntax_for(language).map(|syntax| Lines {
            parse: ParseState::new(syntax),
            highlight: HighlightState::new(&highlighter, ScopeStack::new()),
        });
        Self { lines, highlighter }
    }

    pub fn line(&mut self, line: &str) -> Vec<u32> {
        let Some(lines) = &mut self.lines else { return Vec::new() };
        let mut spans = Vec::new();
        highlight_line(lines, &self.highlighter, &format!("{line}\n"), 0, &mut spans);
        // The line break isn't part of the line.
        let length = line.encode_utf16().count() as u32;
        if let [.., start, len, _] = spans.as_mut_slice() {
            *len = (*len).min(length.saturating_sub(*start));
        }
        spans
    }
}

fn highlight_line(lines: &mut Lines, highlighter: &Highlighter, line: &str, offset: u32, spans: &mut Vec<u32>) {
    if line.len() > MAX_LINE_BYTES {
        return;
    }
    let Ok(operations) = lines.parse.parse_line(line, &SYNTAXES) else { return };
    let mut position = offset;
    for (style, text, _) in RangedHighlightIterator::new(&mut lines.highlight, &operations, line, highlighter) {
        let len = text.encode_utf16().count() as u32;
        let colour = style.foreground.r as u32;
        if colour != 0 && !text.trim().is_empty() {
            push_span(spans, position, len, colour);
        }
        position += len;
    }
}

fn push_span(spans: &mut Vec<u32>, start: u32, len: u32, colour: u32) {
    let count = spans.len();
    if count >= 3 && spans[count - 3] + spans[count - 2] == start && spans[count - 1] == colour {
        spans[count - 2] += len;
        return;
    }
    spans.extend([start, len, colour]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coloured(code: &str, spans: &[u32]) -> Vec<(String, u32)> {
        let units: Vec<u16> = code.encode_utf16().collect();
        let piece = |span: &[u32]| {
            let text = String::from_utf16(&units[span[0] as usize..(span[0] + span[1]) as usize]).unwrap();
            (text, span[2])
        };
        spans.chunks(3).map(piece).collect()
    }

    fn colour_of(code: &str, spans: &[u32], text: &str) -> Option<u32> {
        coloured(code, spans).into_iter().find(|(piece, _)| piece.contains(text)).map(|(_, colour)| colour)
    }

    #[test]
    fn rust_gets_keywords_strings_and_comments() {
        let code = "// say hi\nfn main() {\n    println!(\"hi\");\n}";
        let spans = highlight("rust", code);

        assert_eq!(colour_of(code, &spans, "// say hi"), Some(Colour::Comment as u32));
        assert_eq!(colour_of(code, &spans, "fn"), Some(Colour::Keyword as u32));
        assert_eq!(colour_of(code, &spans, "main"), Some(Colour::Function as u32));
        assert_eq!(colour_of(code, &spans, "hi\""), Some(Colour::String as u32));
    }

    #[test]
    fn common_fence_names_are_known() {
        for language in [
            "ts",
            "tsx",
            "typescript",
            "js",
            "python",
            "py",
            "sh",
            "bash",
            "shell",
            "json",
            "yaml",
            "toml",
            "go",
            "swift",
            "diff",
            "sql",
            "html",
            "css",
            "dockerfile",
            "kotlin",
        ] {
            assert!(syntax_for(language).is_some(), "{language}");
        }
        assert!(syntax_for("text").is_none());
        assert_eq!(*highlight("no-such-language", "x = 1"), Vec::<u32>::new());
    }

    #[test]
    fn streamed_code_gets_the_same_spans_as_the_whole() {
        let code = "def greet(name):\n    \"\"\"Say hello.\n    Twice.\"\"\"\n    print(f\"Hello {name}\")\n\ngreet(\"world\")\n";
        let whole = highlight("python", code);

        let mut incremental = Incremental::new("python");
        let mut streamed = Spans::default();
        let characters: Vec<char> = code.chars().collect();
        for end in (0..=characters.len()).step_by(7).chain([characters.len()]) {
            let so_far: String = characters[..end].iter().collect();
            streamed = incremental.advance(&so_far);
        }

        assert_eq!(streamed, whole);
        assert!(!whole.is_empty());
    }

    #[test]
    fn spans_count_utf16_units() {
        let code = "x = \"😀\"  # done";
        let spans = highlight("python", code);
        assert_eq!(colour_of(code, &spans, "# done"), Some(Colour::Comment as u32));
    }

    #[test]
    fn code_that_changed_underneath_starts_over() {
        let mut incremental = Incremental::new("rust");
        incremental.advance("let a = 1;\nlet b");
        let replaced = incremental.advance("// other\nfn x() {}\n");
        assert_eq!(replaced, highlight("rust", "// other\nfn x() {}\n"));
    }
}
