//! The message protocol of prompt.md §7: a stored response with its `]]>openui:` marker lines
//! (`content`, `context`, `end`).

use std::borrow::Cow;

use indexmap::IndexMap;

use crate::value::Value;

const PREFIX: &str = "]]>openui:";
const CONTENT: &str = "]]>openui:content";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Message {
    /// The body of the last `content` section, or the whole text when there is none.
    pub content: String,
    /// The body of the `context` section after it: JSON when it parses, else the text.
    pub context: Option<Value>,
    /// The attributes of the winning `content` marker.
    pub attributes: IndexMap<String, String>,
    /// An `end` line follows the winning `content` section.
    pub end: bool,
}

struct Section<'a> {
    kind: Option<&'a str>,
    attributes: Option<&'a str>,
    lines: Vec<&'a str>,
}

/// The content of a response, with or without marker lines. While streaming, a last line that
/// may still become a marker is held back.
pub fn content(text: &str, streaming: bool) -> Cow<'_, str> {
    if !text.contains("]]>") {
        return Cow::Borrowed(text);
    }
    Cow::Owned(parse(text, streaming).content)
}

/// Splits a stored message into its content, context, attributes and end flag.
pub fn parse(raw: &str, streaming: bool) -> Message {
    let mut text = split_retries(raw);
    if streaming {
        hold_partial_retry(&mut text);
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    let last = lines.last().copied().unwrap_or_default();
    let held = streaming && !last.is_empty() && (last.starts_with(PREFIX) || PREFIX.starts_with(last));
    if held {
        lines.pop();
    }

    let mut sections = vec![Section { kind: None, attributes: None, lines: Vec::new() }];
    let mut end_at = None;
    let count = lines.len();
    for (index, line) in lines.iter().enumerate() {
        let Some((kind, attributes)) = marker(line.strip_suffix('\r').unwrap_or(line)) else {
            sections.last_mut().expect("a section").lines.push(line);
            continue;
        };
        close(sections.last_mut().expect("a section"));
        if kind == "end" {
            end_at = Some(sections.len() - 1);
            if index + 2 == count && lines[index + 1].is_empty() {
                break;
            }
            continue;
        }
        sections.push(Section { kind: Some(kind), attributes, lines: Vec::new() });
    }
    if held {
        close(sections.last_mut().expect("a section"));
    }

    let winner = sections.iter().rposition(|section| section.kind == Some("content")).unwrap_or(0);
    let context =
        sections.iter().rposition(|section| section.kind == Some("context")).filter(|context| *context > winner).map(
            |context| {
                let body = sections[context].lines.join("\n");
                Value::from_json_str(&body).unwrap_or(Value::String(body))
            },
        );
    let section = &sections[winner];
    Message {
        content: section.lines.join("\n"),
        context,
        attributes: section.attributes.map(read_attributes).unwrap_or_default(),
        end: end_at.is_some_and(|end| end >= winner),
    }
}

/// Writes a stored message with its marker lines.
pub fn build(message: &Message) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in &message.attributes {
        query.append_pair(key, value);
    }
    let query = query.finish();
    let mut out = String::from(CONTENT);
    if !query.is_empty() {
        out.push('?');
        out.push_str(&query);
    }
    out.push('\n');
    out.push_str(&message.content);
    if let Some(context) = &message.context {
        out.push('\n');
        out.push_str(PREFIX);
        out.push_str("context\n");
        match context {
            Value::String(text) => out.push_str(text),
            value => out.push_str(&value.to_json_string()),
        }
    }
    if message.end {
        out.push('\n');
        out.push_str(PREFIX);
        out.push_str("end");
    }
    out
}

/// `]]>openui:kind?attributes`: the kind and the attributes.
fn marker(line: &str) -> Option<(&str, Option<&str>)> {
    let rest = line.strip_prefix(PREFIX)?;
    let (kind, attributes) = match rest.split_once('?') {
        Some((kind, attributes)) => (kind, Some(attributes)),
        None => (rest, None),
    };
    if kind.is_empty() || !kind.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    Some((kind, attributes))
}

/// The newline (and its `\r`) before a marker is not part of the body.
fn close(section: &mut Section<'_>) {
    if let Some(last) = section.lines.last_mut() {
        *last = last.strip_suffix('\r').unwrap_or(last);
    }
}

fn read_attributes(query: &str) -> IndexMap<String, String> {
    url::form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

/// A regenerated response writes its content line right after the cut-off text: it starts a
/// line of its own there, so that the last content wins.
fn split_retries(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut from = 0;
    for (at, _) in raw.match_indices(CONTENT) {
        let after = &raw[at + CONTENT.len()..];
        let ends = after.is_empty() || after.starts_with('?') || after.starts_with('\n') || after.starts_with("\r\n");
        if !ends || at == 0 || raw.as_bytes()[at - 1] == b'\n' {
            continue;
        }
        out.push_str(&raw[from..at]);
        out.push('\n');
        from = at;
    }
    out.push_str(&raw[from..]);
    out
}

/// While streaming, a mid-line `]]>openui:con` that may become a content line is held back.
fn hold_partial_retry(text: &mut String) {
    let Some(at) = text.rfind("]]>") else { return };
    let tail = &text[at..];
    let partial = tail.bytes().all(|b| b.is_ascii_lowercase() || b == b':' || b == b']' || b == b'>');
    if at == 0 || text.as_bytes()[at - 1] == b'\n' || !partial || !CONTENT.starts_with(tail) {
        return;
    }
    text.truncate(at);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_last_content_with_its_context_and_end() {
        let raw = "]]>openui:content?library=support%401.2.0\nroot = Card([title])\ntitle = Text(\"Order\")\n]]>openui:context\n{\"formState\":{\"note\":\"rush\"}}\n]]>openui:end";
        let message = parse(raw, false);
        assert_eq!(message.content, "root = Card([title])\ntitle = Text(\"Order\")");
        assert_eq!(message.attributes.get("library").map(String::as_str), Some("support@1.2.0"));
        assert_eq!(message.context.unwrap().to_json_string(), r#"{"formState":{"note":"rush"}}"#);
        assert!(message.end);
        assert_eq!(build(&parse(raw, false)), raw);
    }

    #[test]
    fn lets_a_regenerated_content_win() {
        let raw = "]]>openui:content\nroot = Car]]>openui:content\nroot = Card([])";
        assert_eq!(parse(raw, false).content, "root = Card([])");
        let earlier = "]]>openui:content\na = 1\n]]>openui:end\n]]>openui:content\nb = 2";
        let message = parse(earlier, false);
        assert_eq!(message.content, "b = 2");
        assert!(!message.end);
    }

    #[test]
    fn holds_back_a_marker_cut_mid_stream() {
        assert_eq!(content("]]>openui:content\na = 1\n]]>open", true), "a = 1");
        assert_eq!(content("]]>openui:content\na = 1\n]]>open", false), "a = 1\n]]>open");
        assert_eq!(content("a = 1\nb = 2", true), "a = 1\nb = 2");
        assert_eq!(content("]]>openui:content\nroot = X]]>openui:con", true), "root = X");
    }
}
