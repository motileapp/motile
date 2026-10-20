//! Preparing the source text (language.md §1.2): the content of the marker lines, the joined
//! fences, comments stripped, trimmed.

use std::borrow::Cow;

use crate::js;
use crate::message;

const FENCE: &str = "```";

/// The program text of a response. While streaming, a marker line that may still be cut is held
/// back.
pub fn prepare(text: &str, streaming: bool) -> String {
    let content = message::content(text, streaming);
    let fenced = extract_fences(js::trim(&content));
    js::trim(&strip_comments(&fenced)).to_owned()
}

/// The contents of every fence, joined with newlines, or the text itself when it has none. A
/// fence opens with three backticks at the start of a line; backticks in a double-quoted string
/// inside it do not close it; one never closed runs to the end.
pub fn extract_fences(input: &str) -> Cow<'_, str> {
    let mut blocks: Vec<&str> = Vec::new();
    let mut from = 0;
    while let Some(open) = find_opener(input, from) {
        let Some(newline) = input[open..].find('\n') else {
            blocks.push("");
            break;
        };
        let body = open + newline + 1;
        match find_closer(input, body) {
            Some(close) => {
                blocks.push(&input[body..close]);
                from = close + FENCE.len();
            }
            None => {
                blocks.push(&input[body..]);
                break;
            }
        }
    }
    if blocks.is_empty() {
        return Cow::Borrowed(input);
    }
    Cow::Owned(blocks.join("\n"))
}

fn find_opener(input: &str, from: usize) -> Option<usize> {
    let mut at = from;
    loop {
        let found = at + input[at..].find(FENCE)?;
        if found == 0 || input.as_bytes()[found - 1] == b'\n' {
            return Some(found);
        }
        at = found + 1;
    }
}

fn find_closer(input: &str, from: usize) -> Option<usize> {
    let bytes = input.as_bytes();
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => at = skip_string(bytes, at),
            b'`' if input[at..].starts_with(FENCE) => return Some(at),
            _ => at += 1,
        }
    }
    None
}

/// Past a double-quoted string that starts at `at`, or the end when it never closes.
fn skip_string(bytes: &[u8], at: usize) -> usize {
    let mut at = at + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// Removes `//` and `#` comments, which run to the end of their line, outside strings. A string
/// is tracked as the lexer reads it, so a backslash escapes the next character, a newline too.
pub fn strip_comments(input: &str) -> Cow<'_, str> {
    if !input.contains('#') && !input.contains("//") {
        return Cow::Borrowed(input);
    }
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut quote: Option<u8> = None;
    let mut copied = 0;
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        if let Some(open) = quote {
            match byte {
                b'\\' => at += 1,
                byte if byte == open => quote = None,
                _ => {}
            }
            at += 1;
            continue;
        }
        let comment = byte == b'#' || (byte == b'/' && bytes.get(at + 1) == Some(&b'/'));
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
        } else if comment {
            out.push_str(input[copied..at].trim_end_matches([' ', '\t', '\r']));
            at = input[at..].find('\n').map_or(bytes.len(), |newline| at + newline);
            copied = at;
            continue;
        }
        at += 1;
    }
    out.push_str(&input[copied.min(input.len())..]);
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_fences_that_open_at_a_line_start() {
        let text = "First part:\n```openui\nroot = Stack([a, b])\na = Text(\"use ``` here\")\n```\nAnd the rest, in a fence that is never closed:\n```openui\nb = Text(\"B\")\n";
        assert_eq!(prepare(text, false), "root = Stack([a, b])\na = Text(\"use ``` here\")\n\nb = Text(\"B\")");
        assert_eq!(prepare("Say ```x``` inline\nroot = A()", false), "Say ```x``` inline\nroot = A()");
        assert_eq!(prepare("Don't \"worry\n```\nroot = A()\n```", false), "root = A()");
        assert_eq!(prepare("```openui", true), "");
    }

    #[test]
    fn strips_comments_outside_strings() {
        let text = "root = Stack([t]) // the page\n# a note line\nt = Text(\"a // not\")\nu = Text('b # not')\nv = Text(\"one\n// in\")";
        assert_eq!(
            prepare(text, false),
            "root = Stack([t])\n\nt = Text(\"a // not\")\nu = Text('b # not')\nv = Text(\"one\n// in\")"
        );
        assert_eq!(strip_comments("a = \"x\\\"#\" # c"), "a = \"x\\\"#\"");
    }
}
