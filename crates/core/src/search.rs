//! The sidebar's search through the threads' messages: what each server found, kept while the
//! search changes, and its excerpts in parts that set off what was searched for.

use motile_protocol::search::{Folded, Matcher, ranges};
use motile_protocol::wire::{Speaker, ThreadMatch};
use serde::Serialize;

pub use motile_protocol::search::searchable;

/// How many threads each server answers with, at most.
pub const LIMIT: u32 = 50;

/// What a server found in a thread.
#[derive(Clone, Debug)]
pub struct Found {
    pub server_id: String,
    /// The server that answered: `server_id`, or the one holding its copy once it was removed.
    pub asked: String,
    pub thread: ThreadMatch,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct MatchView {
    pub server_id: String,
    pub thread_id: String,
    pub speaker: Speaker,
    pub parts: Vec<Part>,
    /// The message it was found by, which the thread opens at.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub item_id: String,
    /// The search is in the message as it was written, so the thread opens finding it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub phrase: bool,
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

/// Keeps what was found that still fits the search while it changes.
pub fn keep_fitting(found: &mut Vec<Found>, query: &str) {
    let matcher = Matcher::new(query);
    found.retain(|found| matcher.grade(&found.thread.excerpt).is_some());
}

/// What `asked` answered, into what shows: what it no longer finds goes, what it finds again
/// stays in its place, and what it finds anew comes below, so that nothing shown moves.
pub fn merge(shown: &mut Vec<Found>, asked: &str, answer: Vec<Found>) {
    let mut answer: Vec<Option<Found>> = answer.into_iter().map(Some).collect();
    shown.retain_mut(|found| {
        if found.asked != asked {
            return true;
        }
        let same =
            |again: &Found| again.server_id == found.server_id && again.thread.thread_id == found.thread.thread_id;
        let again = answer.iter_mut().find(|again| again.as_ref().is_some_and(same)).and_then(Option::take);
        let Some(again) = again else { return false };
        *found = again;
        true
    });
    shown.extend(answer.into_iter().flatten());
}

pub fn view(found: &Found, query: &str) -> MatchView {
    MatchView {
        server_id: found.server_id.clone(),
        thread_id: found.thread.thread_id.clone(),
        speaker: found.thread.speaker,
        parts: parts(&found.thread.excerpt, query),
        item_id: found.thread.item_id.clone(),
        phrase: !Folded::new(&found.thread.excerpt).find(&Folded::new(query)).is_empty(),
    }
}

/// The excerpt cut where the words of `query` are, each time they are there.
pub fn parts(excerpt: &str, query: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut at = 0;
    for range in ranges(excerpt, query) {
        if range.start > at {
            parts.push(Part { text: excerpt[at..range.start].to_string(), matched: false });
        }
        parts.push(Part { text: excerpt[range.clone()].to_string(), matched: true });
        at = range.end;
    }
    if at < excerpt.len() {
        parts.push(Part { text: excerpt[at..].to_string(), matched: false });
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(text: &str, matched: bool) -> Part {
        Part { text: text.into(), matched }
    }

    #[test]
    fn an_excerpt_sets_off_every_word_whatever_its_case_and_accents() {
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
        assert_eq!(parts("Green café", "cafe green"), [part("Green", true), part(" ", false), part("café", true)]);
        assert_eq!(parts("two drafts", "fts"), [part("two drafts", false)]);
    }

    fn found(asked: &str, thread_id: &str, excerpt: &str) -> Found {
        let thread = ThreadMatch {
            thread_id: thread_id.into(),
            speaker: Speaker::User,
            excerpt: excerpt.into(),
            item_id: String::new(),
            origin: None,
        };
        Found { server_id: asked.into(), asked: asked.into(), thread }
    }

    #[test]
    fn what_a_server_answers_later_comes_below_and_what_shows_stays_in_place() {
        let mut shown = vec![found("a", "1", "deploy"), found("b", "2", "deploy"), found("a", "3", "deploy")];
        keep_fitting(&mut shown, "deploy it");
        assert!(shown.is_empty());

        let mut shown = vec![found("a", "1", "deploy"), found("b", "2", "deploy"), found("a", "3", "deploy")];
        merge(&mut shown, "a", vec![found("a", "4", "new"), found("a", "3", "deploy again")]);

        let shown: Vec<_> =
            shown.iter().map(|found| (found.thread.thread_id.as_str(), found.thread.excerpt.as_str())).collect();
        assert_eq!(shown, [("2", "deploy"), ("3", "deploy again"), ("4", "new")]);
    }
}
