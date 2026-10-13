//! The sidebar's search through the threads' messages: what each server found, kept while the
//! search changes, and its excerpts in parts that set off what was searched for.

use motile_protocol::wire::{Speaker, ThreadMatch};
use serde::Serialize;

/// How many characters a search needs before the servers are asked.
pub const MIN_QUERY: usize = 2;
/// How many threads each server answers with, at most.
pub const LIMIT: u32 = 50;

/// What a server found in a thread.
#[derive(Clone, Debug)]
pub struct Found {
    pub server_id: String,
    pub thread: ThreadMatch,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MatchView {
    pub server_id: String,
    pub thread_id: String,
    pub speaker: Speaker,
    pub parts: Vec<Part>,
}

/// A piece of an excerpt, `matched` where it is what was searched for.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Part {
    pub text: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub matched: bool,
}

/// The search as the client typed it, on one line without the spaces around it.
pub fn normalized(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `query` is in `text`, with letters of any case, as the servers search.
pub fn contains(text: &str, query: &str) -> bool {
    find(text, query).is_some()
}

pub fn view(found: &Found, query: &str) -> MatchView {
    MatchView {
        server_id: found.server_id.clone(),
        thread_id: found.thread.thread_id.clone(),
        speaker: found.thread.speaker,
        parts: parts(&found.thread.excerpt, query),
    }
}

/// The excerpt cut where `query` starts and ends, each time it is there.
pub fn parts(excerpt: &str, query: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut rest = excerpt;
    while let Some(start) = find(rest, query) {
        let end = start + query.len();
        if start > 0 {
            parts.push(Part { text: rest[..start].to_string(), matched: false });
        }
        parts.push(Part { text: rest[start..end].to_string(), matched: true });
        rest = &rest[end..];
    }
    if !rest.is_empty() {
        parts.push(Part { text: rest.to_string(), matched: false });
    }
    parts
}

/// Where `query` first is in `text`, with letters of any case. Only ASCII letters are folded, as
/// SQLite's `LIKE` does, so a match starts and ends between characters.
fn find(text: &str, query: &str) -> Option<usize> {
    let (text, query) = (text.as_bytes(), query.as_bytes());
    if query.is_empty() || query.len() > text.len() {
        return None;
    }
    text.windows(query.len()).position(|window| window.eq_ignore_ascii_case(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(text: &str, matched: bool) -> Part {
        Part { text: text.into(), matched }
    }

    #[test]
    fn an_excerpt_sets_off_every_match_whatever_its_case() {
        assert_eq!(
            parts("…Deploy it, then deploy é again", "deploy"),
            [
                part("…", false),
                part("Deploy", true),
                part(" it, then ", false),
                part("deploy", true),
                part(" é again", false)
            ]
        );
        assert_eq!(parts("café crème", "É"), [part("café crème", false)]);
        assert_eq!(parts("café", "fé"), [part("ca", false), part("fé", true)]);
    }

    #[test]
    fn a_search_is_one_line_without_spaces_around_it() {
        assert_eq!(normalized("  watch\nthe   deploy "), "watch the deploy");
        assert!(contains("Watch the DEPLOY", "the deploy"));
        assert!(!contains("Watch", ""));
    }
}
