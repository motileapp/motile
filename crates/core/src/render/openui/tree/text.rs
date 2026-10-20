//! Text as the clients set it: open-intelligent-ui's inline formatting (`markdown.tsx`'s `inline`:
//! `` `code` ``, `**bold**`, `~~strike~~`, `*italic*` and `[links](https://…)`) as the runs prose
//! rows use, and its white space as CSS collapses it.

use std::sync::LazyLock;

use regress::Regex;
use url::Url;

use super::Rich;
use crate::render::markdown::{BOLD, CODE, ITALIC, LINK, Link, STRIKE, utf16_len};

/// Stands for an inline component in a Text's text.
pub const OBJECT: char = '\u{fffc}';

/// `markdown.tsx`'s `INLINE`, with a line break as U+2028 too.
static INLINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(`[^`\n ]+`)|(\*\*[^*\n ]+?\*\*|__[^_\n ]+?__)|(~~[^~\n ]+?~~)|(\*[^*\s][^*\n ]*?\*|_[^_\s][^_\n ]*?_)|(\[[^\]\n ]+\]\([^)\s]+\))",
    )
    .expect("a valid pattern")
});

/// An absolute http(s) URL (`webUrl`).
pub fn web_url(value: &str) -> Option<String> {
    let value = value.trim();
    let url = Url::parse(value).ok()?;
    matches!(url.scheme(), "http" | "https").then(|| value.to_owned())
}

/// An absolute https URL: what a client fetches itself, as HtmlView's policy lets a page.
pub fn https_url(value: &str) -> Option<String> {
    let value = value.trim();
    let url = Url::parse(value).ok()?;
    (url.scheme() == "https").then(|| value.to_owned())
}

/// White space as `white-space: normal` sets it: each run of it one space, none at either end.
pub fn collapse(text: &str) -> String {
    text.split([' ', '\t', '\n', '\r', '\u{c}']).filter(|word| !word.is_empty()).collect::<Vec<_>>().join(" ")
}

/// White space as `white-space: pre-line` sets it: line breaks kept, as U+2028, each run of
/// spaces one space, none around a break. Spaces at either end stay, for the text around it.
pub fn collapse_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for character in text.replace("\r\n", "\n").chars() {
        match character {
            ' ' | '\t' => space = true,
            '\n' | '\r' => {
                space = false;
                out.truncate(out.trim_end_matches(' ').len());
                out.push('\u{2028}');
            }
            character => {
                if std::mem::take(&mut space) && !out.ends_with('\u{2028}') {
                    out.push(' ');
                }
                out.push(character);
            }
        }
    }
    if space && !out.ends_with('\u{2028}') {
        out.push(' ');
    }
    out
}

/// A block's text without the white space at its ends.
pub fn trim_block(text: &str) -> &str {
    text.trim_start_matches(' ').trim_end_matches([' ', '\u{2028}'])
}

#[derive(Default)]
pub struct Builder {
    rich: Rich,
    length: u32,
}

impl Builder {
    pub fn finish(self) -> Rich {
        self.rich
    }

    fn push(&mut self, text: &str, style: u32) {
        if text.is_empty() {
            return;
        }
        let start = self.length;
        let length = utf16_len(text);
        self.rich.text.push_str(text);
        self.length += length;
        if style == 0 {
            return;
        }
        match self.rich.runs.last_mut() {
            Some(run) if run[0] + run[1] == start && run[2] == style => run[1] += length,
            _ => self.rich.runs.push([start, length, style]),
        }
    }

    /// The character an inline component takes in the text.
    pub fn object(&mut self) {
        self.push(&OBJECT.to_string(), 0);
    }

    pub fn plain(&mut self, text: &str) {
        self.push(text, 0);
    }

    /// Text with its inline formatting read.
    pub fn inline(&mut self, text: &str, style: u32) {
        let mut rest = text;
        while !rest.is_empty() {
            let Some(found) = INLINE.find(rest) else {
                self.push(rest, style);
                return;
            };
            let range = found.range();
            self.push(&rest[..range.start], style);
            let token = &rest[range.clone()];
            let group = (1..=5).find(|group| found.group(*group).is_some()).unwrap_or(5);
            match group {
                1 => self.push(&token[1..token.len() - 1], style | CODE),
                2 => self.inline(&token[2..token.len() - 2], style | BOLD),
                3 => self.inline(&token[2..token.len() - 2], style | STRIKE),
                4 => self.inline(&token[1..token.len() - 1], style | ITALIC),
                _ => self.link(token, style),
            }
            rest = &rest[range.end..];
        }
    }

    fn link(&mut self, token: &str, style: u32) {
        let close = token.find("](").unwrap_or(token.len() - 1);
        let label = &token[1..close];
        let Some(url) = web_url(&token[close + 2..token.len() - 1]) else {
            self.inline(label, style);
            return;
        };
        let start = self.length;
        self.inline(label, style | LINK);
        if self.length > start {
            self.rich.links.push(Link { start, len: self.length - start, url });
        }
    }
}

/// A line of text with its inline formatting, as `white-space: normal` sets it.
pub fn inline(text: &str) -> Rich {
    let mut builder = Builder::default();
    builder.inline(&collapse(text), 0);
    builder.finish()
}

pub fn plain(text: &str) -> Rich {
    let mut builder = Builder::default();
    builder.plain(text);
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(rich: &Rich) -> Vec<(String, u32)> {
        let units: Vec<u16> = rich.text.encode_utf16().collect();
        rich.runs
            .iter()
            .map(|run| (String::from_utf16(&units[run[0] as usize..(run[0] + run[1]) as usize]).unwrap(), run[2]))
            .collect()
    }

    #[test]
    fn reads_open_intelligent_uis_inline_grammar() {
        let rich = inline("Use `npm i`, **bold _and it_**, ~~gone~~, *soft* and [docs](https://openui.com).");
        assert_eq!(rich.text, "Use npm i, bold and it, gone, soft and docs.");
        assert_eq!(
            styled(&rich),
            [
                ("npm i".into(), CODE),
                ("bold ".into(), BOLD),
                ("and it".into(), BOLD | ITALIC),
                ("gone".into(), STRIKE),
                ("soft".into(), ITALIC),
                ("docs".into(), LINK),
            ]
        );
        assert_eq!(rich.links, [Link { start: 39, len: 4, url: "https://openui.com".into() }]);
    }

    #[test]
    fn links_only_to_the_web() {
        let rich = inline("[run](javascript:alert(1)) [file](file:///etc) [**site**](http://x.dev)");
        assert_eq!(rich.text, "run) file site");
        assert_eq!(rich.links, [Link { start: 10, len: 4, url: "http://x.dev".into() }]);
        assert_eq!(styled(&rich), [("site".into(), BOLD | LINK)]);
    }

    #[test]
    fn leaves_unclosed_and_spaced_markers_as_they_are() {
        assert_eq!(inline("2 * 3 * 4 and **open").text, "2 * 3 * 4 and **open");
        assert!(inline("a * b*").runs.is_empty());
    }

    #[test]
    fn counts_utf16_units() {
        let rich = inline("😀 **b** [l](https://a.b)");
        assert_eq!(rich.runs, [[3, 1, BOLD], [5, 1, LINK]]);
    }

    #[test]
    fn collapses_white_space_as_css_does() {
        assert_eq!(collapse("  a \n\t b  "), "a b");
        assert_eq!(collapse_lines("a   b \n  c\n\n d\n"), "a b\u{2028}c\u{2028}\u{2028}d\u{2028}");
        assert_eq!(collapse_lines("  25   min · "), " 25 min · ");
        assert_eq!(trim_block(" a\u{2028}b \u{2028}"), "a\u{2028}b");
    }

    #[test]
    fn checks_web_addresses() {
        assert_eq!(web_url(" https://bea.gov "), Some("https://bea.gov".into()));
        assert_eq!(web_url("javascript:alert(1)"), None);
        assert_eq!(web_url("/relative"), None);
        assert_eq!(web_url("mailto:a@b.c"), None);
        assert_eq!(https_url(" https://bea.gov/a.png "), Some("https://bea.gov/a.png".into()));
        assert_eq!(https_url("http://192.168.1.1/reboot"), None);
        assert_eq!(https_url("HTTPS://bea.gov"), Some("HTTPS://bea.gov".into()));
        assert_eq!(https_url("file:///etc/passwd"), None);
    }
}
