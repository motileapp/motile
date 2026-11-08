//! Finding in the open thread, as a browser finds in a page: a phrase as it is written, whatever
//! its case and accents, through every message the thread has and not only the turns loaded. Each
//! row of a message is searched as it reads: the user's text, a reply's prose without its
//! Markdown, its code and its errors.

use motile_protocol::search::Folded;
use motile_protocol::wire::{Item, ItemKind, PLAN_TOOL};
use serde::Serialize;

use crate::render::rows::{self, Row};

/// A row's text, ready to be searched.
pub struct Entry {
    seq: u64,
    block: usize,
    row_id: String,
    item_id: String,
    text: String,
    folded: Folded,
    /// Where the phrase is in the text, `[start, length]` in UTF-16 units.
    hits: Vec<[u32; 2]>,
}

/// Where a row stands in the thread: its item's place, then its own among the item's rows.
pub type Place = (u64, usize);

/// A point in the thread's text: a row's place, and an offset in its text in UTF-16 units.
pub type At = (u64, usize, u32);

/// A match: its row and where it is in the row's text, in UTF-16 units.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Hit {
    pub row_id: String,
    pub item_id: String,
    #[serde(skip)]
    pub seq: u64,
    pub start: u32,
    pub len: u32,
}

/// The matches in one row the client has.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct FoundRow {
    pub row_id: String,
    /// `[start, length]` in UTF-16 units.
    pub ranges: Vec<[u32; 2]>,
    /// Which of them is the current match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<usize>,
}

#[derive(Default)]
pub struct Find {
    query: String,
    phrase: Option<Folded>,
    entries: Vec<Entry>,
    /// The current match by its row and start, which stays on it while what is around it changes.
    current: Option<(String, u32)>,
    /// Whether the thread's rows have been read.
    pub ready: bool,
    /// Where the search started, for once the thread's rows have been read.
    pub from: Option<At>,
    pub item: Option<String>,
}

/// The rows of `items` that are searched, in order.
pub fn index(items: &[Item], cwd: &str) -> Vec<Entry> {
    let mut entries = Vec::new();
    for item in items.iter().filter(|item| searched(item)) {
        for row in rows::rendered(item, cwd) {
            entries.extend(entry(item.seq, &row));
        }
    }
    entries
}

fn searched(item: &Item) -> bool {
    match &item.kind {
        ItemKind::User { .. } | ItemKind::Assistant { .. } | ItemKind::Error { .. } => true,
        ItemKind::Tool { call } => call.name == PLAN_TOOL,
        _ => false,
    }
}

fn entry(seq: u64, row: &Row) -> Option<Entry> {
    let text = rows::searched_text(row)?.to_string();
    Some(Entry {
        seq,
        block: block(&row.id),
        row_id: row.id.clone(),
        item_id: row.item.clone(),
        folded: Folded::new(&text),
        text,
        hits: Vec::new(),
    })
}

/// A row's place among its item's rows, from its id: `<item>/<index>`.
pub fn block(row_id: &str) -> usize {
    row_id.rsplit_once('/').and_then(|(_, index)| index.parse().ok()).unwrap_or(0)
}

impl Find {
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Takes the thread's rows once they are read, and keeps the current match where it was.
    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        self.ready = true;
        self.match_all();
    }

    /// Searches for `query`. With `item`, the message the sidebar's search found, the current match
    /// is its first, once the whole thread is searched, and none when it has none. Without, it is
    /// the first at or after the current match, or without one `from`, round from the start.
    /// Whether it moved.
    pub fn search(&mut self, query: &str, from: Option<At>, item: Option<&str>) -> bool {
        let before = self.current.clone();
        let from = self.current_at().or(from);
        self.query = query.to_string();
        self.phrase = Some(Folded::new(query.trim())).filter(|phrase| !phrase.is_empty());
        self.match_all();
        let hits = self.hits();
        if let Some(item) = item {
            let in_item = hits.iter().find(|hit| hit.item_id == item).filter(|_| self.ready);
            self.current = in_item.map(|hit| (hit.row_id.clone(), hit.start));
            return self.current != before;
        }
        let next = {
            let from = from.unwrap_or((0, 0, 0));
            let at = |hit: &&Hit| self.at_of(hit).is_some_and(|at| at >= from);
            hits.iter().find(at).or(hits.first())
        };
        self.current = next.map(|hit| (hit.row_id.clone(), hit.start));
        self.current != before
    }

    /// Moves to the next match, or with `back` the one before, round from either end.
    pub fn step(&mut self, back: bool) -> bool {
        let hits = self.hits();
        if hits.is_empty() {
            return false;
        }
        let count = hits.len();
        let next = match self.current_index() {
            Some(at) if back => (at + count - 1) % count,
            Some(at) => (at + 1) % count,
            None if back => count - 1,
            None => 0,
        };
        self.current = Some((hits[next].row_id.clone(), hits[next].start));
        true
    }

    /// Takes a row as it is now, after it grew or came in. The current match stays on its place.
    pub fn update(&mut self, seq: u64, row: &Row) {
        let Some(mut new) = entry(seq, row) else { return };
        if let Some(phrase) = &self.phrase {
            new.hits = hits_in(&new.text, &new.folded, phrase);
        }
        let key = (new.seq, new.block);
        match self.entries.iter().position(|entry| entry.row_id == new.row_id) {
            Some(index) if self.entries[index].text == new.text => {}
            Some(index) => self.entries[index] = new,
            None => {
                let at = self.entries.partition_point(|entry| (entry.seq, entry.block) < key);
                self.entries.insert(at, new);
            }
        }
        self.settle_current();
    }

    /// Every match, in the thread's order.
    pub fn hits(&self) -> Vec<Hit> {
        let hits = self.entries.iter().flat_map(|entry| {
            entry.hits.iter().map(|[start, len]| Hit {
                row_id: entry.row_id.clone(),
                item_id: entry.item_id.clone(),
                seq: entry.seq,
                start: *start,
                len: *len,
            })
        });
        hits.collect()
    }

    pub fn count(&self) -> usize {
        self.entries.iter().map(|entry| entry.hits.len()).sum()
    }

    pub fn current_index(&self) -> Option<usize> {
        let (row_id, start) = self.current.as_ref()?;
        let mut index = 0;
        for entry in &self.entries {
            if entry.row_id == *row_id {
                return entry.hits.iter().position(|hit| hit[0] == *start).map(|at| index + at);
            }
            index += entry.hits.len();
        }
        None
    }

    pub fn current(&self) -> Option<Hit> {
        let (row_id, start) = self.current.as_ref()?;
        let entry = self.entries.iter().find(|entry| entry.row_id == *row_id)?;
        let [start, len] = *entry.hits.iter().find(|hit| hit[0] == *start)?;
        Some(Hit { row_id: row_id.clone(), item_id: entry.item_id.clone(), seq: entry.seq, start, len })
    }

    /// The matches in the rows the client has.
    pub fn found_rows(&self, rows: &[Row]) -> Vec<FoundRow> {
        let shown: std::collections::HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        let current = self.current.as_ref();
        let found = self.entries.iter().filter(|entry| !entry.hits.is_empty() && shown.contains(entry.row_id.as_str()));
        found
            .map(|entry| FoundRow {
                row_id: entry.row_id.clone(),
                ranges: entry.hits.clone(),
                current: current
                    .filter(|(row_id, _)| *row_id == entry.row_id)
                    .and_then(|(_, start)| entry.hits.iter().position(|hit| hit[0] == *start)),
            })
            .collect()
    }

    fn match_all(&mut self) {
        for entry in &mut self.entries {
            entry.hits = match &self.phrase {
                Some(phrase) => hits_in(&entry.text, &entry.folded, phrase),
                None => Vec::new(),
            };
        }
        self.settle_current();
    }

    /// Keeps the current match on its place, or the first match after it once it is gone.
    fn settle_current(&mut self) {
        let Some((row_id, start)) = self.current.clone() else { return };
        let still = self
            .entries
            .iter()
            .find(|entry| entry.row_id == row_id)
            .is_some_and(|entry| entry.hits.iter().any(|hit| hit[0] == start));
        if still {
            return;
        }
        let from = self.place_of(&row_id).map(|(seq, block)| (seq, block, start));
        let hits = self.hits();
        let after = |hit: &&Hit| from.is_none_or(|from| self.at_of(hit) >= Some(from));
        let next = hits.iter().find(after).or(hits.first());
        self.current = next.map(|hit| (hit.row_id.clone(), hit.start));
    }

    fn current_at(&self) -> Option<At> {
        let (row_id, start) = self.current.as_ref()?;
        self.place_of(row_id).map(|(seq, block)| (seq, block, *start))
    }

    fn at_of(&self, hit: &Hit) -> Option<At> {
        self.place_of(&hit.row_id).map(|(seq, block)| (seq, block, hit.start))
    }

    fn place_of(&self, row_id: &str) -> Option<Place> {
        self.entries.iter().find(|entry| entry.row_id == row_id).map(|entry| (entry.seq, entry.block))
    }
}

/// Where the phrase is in a row's text, in UTF-16 units.
fn hits_in(text: &str, folded: &Folded, phrase: &Folded) -> Vec<[u32; 2]> {
    let starts = folded.find(phrase);
    if starts.is_empty() {
        return Vec::new();
    }
    let mut units = Vec::with_capacity(folded.len() + 1);
    let mut at = 0;
    for character in text.chars() {
        units.push(at);
        at += character.len_utf16() as u32;
    }
    units.push(at);
    let end = |start: usize| units[(start + phrase.len()).min(units.len() - 1)];
    starts.into_iter().map(|start| [units[start], end(start) - units[start]]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, seq: u64, mut payload: serde_json::Value) -> Item {
        payload["id"] = id.into();
        payload["seq"] = seq.into();
        payload["rev"] = seq.into();
        payload["created_at"] = (seq as f64).into();
        serde_json::from_value(payload).unwrap()
    }

    fn user(id: &str, seq: u64, text: &str) -> Item {
        item(id, seq, serde_json::json!({"type": "user", "text": text, "attachments": []}))
    }

    fn reply(id: &str, seq: u64, text: &str) -> Item {
        item(id, seq, serde_json::json!({"type": "assistant", "text": text}))
    }

    fn find(items: &[Item], query: &str) -> Find {
        let mut find = Find::default();
        find.set_entries(index(items, "/tmp"));
        find.search(query, None, None);
        find
    }

    #[test]
    fn finds_the_phrase_as_it_reads_whatever_its_case_and_accents() {
        let items = [user("a", 1, "Café au lait"), reply("b", 2, "A **CAFE** with `cafe` in it")];
        let find = find(&items, "cafe");
        let hits: Vec<(String, u32, u32)> =
            find.hits().into_iter().map(|hit| (hit.row_id, hit.start, hit.len)).collect();
        assert_eq!(hits, [("a/0".into(), 0, 4), ("b/0".into(), 2, 4), ("b/0".into(), 12, 4)]);
    }

    #[test]
    fn counts_in_utf16_units() {
        let find = find(&[user("a", 1, "😀 the deploy")], "deploy");
        let hit = find.current().unwrap();
        assert_eq!((hit.start, hit.len), (7, 6));
    }

    #[test]
    fn only_searches_what_reads_as_a_message() {
        let items = [item("t", 1, serde_json::json!({"type": "thinking", "text": "deploy"})), reply("b", 2, "No")];
        assert_eq!(find(&items, "deploy").count(), 0);
    }

    #[test]
    fn starts_at_the_first_match_from_where_the_client_is() {
        let items = [user("a", 1, "deploy"), user("b", 2, "deploy"), user("c", 3, "deploy")];
        let mut find = Find::default();
        find.set_entries(index(&items, "/tmp"));
        find.search("depl", Some((2, 0, 0)), None);
        assert_eq!(find.current_index(), Some(1));
        find.search("deploy", Some((1, 0, 0)), None);
        assert_eq!(find.current_index(), Some(1), "a longer search keeps the current match");
        find.search("deploy", None, Some("c"));
        assert_eq!(find.current_index(), Some(2), "the message it was found by comes first");
    }

    #[test]
    fn starts_where_the_client_is_in_a_long_row() {
        let items = [reply("a", 1, "not one, not two, not three"), user("b", 2, "not four")];
        let mut find = Find::default();
        find.set_entries(index(&items, "/tmp"));
        find.search("n", Some((1, 0, 9)), None);
        assert_eq!(find.current().map(|hit| hit.start), Some(9), "not the row's first match");
        find.search("no", Some((1, 0, 0)), None);
        assert_eq!(find.current().map(|hit| hit.start), Some(9), "a longer search stays on the current match");
        find.search("not th", Some((1, 0, 0)), None);
        assert_eq!(find.current().map(|hit| hit.start), Some(18), "and moves on from it when it is gone");
        let mut find = Find::default();
        find.set_entries(index(&items, "/tmp"));
        find.search("not", Some((1, 0, 25)), None);
        assert_eq!(find.current().map(|hit| hit.row_id), Some("b/0".into()), "past the row's last match");
    }

    #[test]
    fn the_message_it_was_found_by_has_the_current_match_or_none() {
        let items = [user("a", 1, "deploy it"), user("b", 2, "it was a deploy")];
        let mut find = Find::default();
        find.search("deploy", None, Some("b"));
        assert_eq!(find.current_index(), None, "it waits for the whole thread");
        find.set_entries(index(&items, "/tmp"));
        find.search("deploy", None, Some("b"));
        assert_eq!(find.current_index(), Some(1));
        find.search("deploy it", None, Some("b"));
        assert_eq!((find.count(), find.current_index()), (1, None), "the words are apart in that message");
    }

    #[test]
    fn steps_round_from_either_end() {
        let mut find = find(&[user("a", 1, "go go"), user("b", 2, "go")], "go");
        assert_eq!(find.current_index(), Some(0));
        find.step(true);
        assert_eq!(find.current_index(), Some(2));
        find.step(false);
        assert_eq!(find.current_index(), Some(0));
    }

    #[test]
    fn keeps_the_current_match_while_a_reply_grows() {
        let mut find = find(&[user("a", 1, "deploy"), reply("b", 2, "the deploy")], "deploy");
        find.step(false);
        let mut grown = rows::rendered(&reply("b", 2, "the deploy is green, deploy again"), "/tmp");
        find.update(2, &grown.remove(0));
        assert_eq!(find.count(), 3);
        assert_eq!(find.current().map(|hit| (hit.row_id, hit.start)), Some(("b/0".into(), 4)));
    }

    #[test]
    fn says_the_matches_of_the_rows_the_client_has() {
        let items = [user("a", 1, "deploy"), user("b", 2, "deploy deploy")];
        let find = find(&items, "deploy");
        let shown = rows::rendered(&items[1], "/tmp");
        let found = find.found_rows(&shown);
        assert_eq!(found, [FoundRow { row_id: "b/0".into(), ranges: vec![[0, 6], [7, 6]], current: None }]);
    }
}
