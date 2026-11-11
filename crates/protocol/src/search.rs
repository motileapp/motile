//! How a search through the threads' messages matches, alike on the servers and the clients: its
//! words in any order, whatever their case and accents. A message is graded by how its words sit
//! (whole, starting a word, inside one), and gets a bonus where they are written together.

use std::ops::Range;

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// A word shorter than this is too short for the servers' index: it only narrows what the
/// others found. A search needs one at least this long.
pub const TOKEN_MIN: usize = 3;
/// A word shorter than this must start a word of the message: "fts" is not in "drafts".
const WORD_START_MIN: usize = 4;

pub const WHOLE_WORD: f64 = 1.0;
/// A word of the search only starts one of the message: "mode" in "model".
pub const PREFIX: f64 = 0.8;
const MID_WORD: f64 = 0.3;
/// The words written together as typed.
const PHRASE_BONUS: f64 = 0.25;
/// The words within about what a result's row shows.
const NEAR_BONUS: f64 = 0.1;
const NEAR_SPAN: usize = 60;

/// How much of a message comes before what was found in its excerpt, at most.
const EXCERPT_LEAD: usize = 24;
pub const EXCERPT_LENGTH: usize = 240;

const QUOTES: &[char] = &['"', '\'', '`', '“', '”', '‘', '’'];

/// The words of a search, lowercased: those long enough for the index, the shorter ones, and all
/// of them as typed.
#[derive(Debug, PartialEq)]
pub struct Tokens {
    pub long: Vec<String>,
    pub short: Vec<String>,
    pub words: Vec<String>,
}

pub fn tokens(query: &str) -> Tokens {
    let unquoted = query.trim().trim_matches(QUOTES);
    let straight = unquoted.replace(['‘', '’'], "'").nfc().collect::<String>().to_lowercase();
    let words: Vec<String> = straight.split_whitespace().map(str::to_string).collect();
    let (long, short) = words.iter().cloned().partition(|word| word.chars().count() >= TOKEN_MIN);
    Tokens { long, short, words }
}

/// Whether the search has a word the servers can look up.
pub fn searchable(query: &str) -> bool {
    !tokens(query).long.is_empty()
}

/// Where a message matches and how well.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Graded {
    /// `WHOLE_WORD` when every word is whole, `PREFIX` when one only starts a word, `MID_WORD`
    /// when one is only inside words, plus the bonus for the words together or near.
    pub quality: f64,
    /// Where its excerpt starts to show the match, in bytes.
    pub offset: usize,
}

struct Token {
    folded: Vec<char>,
    must_start_word: bool,
    /// Long enough to grade the message by. A shorter one only narrows, so that the results
    /// don't reorder while the next word is typed.
    grades: bool,
}

/// A search, prepared once to grade many messages.
pub struct Matcher {
    tokens: Vec<Token>,
    words: Vec<Vec<char>>,
    long: usize,
}

impl Matcher {
    pub fn new(query: &str) -> Self {
        let Tokens { long, short, words } = tokens(query);
        let token = |word: &String, grades: bool| Token {
            folded: fold(word),
            must_start_word: word.chars().count() < WORD_START_MIN,
            grades,
        };
        let tokens = long.iter().map(|word| token(word, true)).chain(short.iter().map(|word| token(word, false)));
        Self { tokens: tokens.collect(), words: words.iter().map(|word| fold(word)).collect(), long: long.len() }
    }

    /// How well `text` matches, or nothing when a word is missing or a short one never starts a
    /// word.
    pub fn grade(&self, text: &str) -> Option<Graded> {
        let chars: Vec<char> = text.chars().collect();
        let folded: Vec<char> = chars.iter().map(|&character| fold_char(character)).collect();
        let mut quality = WHOLE_WORD;
        let mut places = Vec::new();
        for token in &self.tokens {
            let (at, found) = find_token(&chars, &folded, &token.folded)?;
            if found == MID_WORD && token.must_start_word {
                return None;
            }
            if token.grades {
                quality = quality.min(found);
            }
            places.push(at);
        }
        let byte = |index: usize| chars[..index].iter().map(|character| character.len_utf8()).sum();
        if let Some(start) = phrase(&folded, &self.words) {
            return Some(Graded { quality: quality + PHRASE_BONUS, offset: byte(start) });
        }
        let (start, length) = shortest_span(&places);
        let near = self.long > 1 && length <= NEAR_SPAN;
        Some(Graded { quality: if near { quality + NEAR_BONUS } else { quality }, offset: byte(start) })
    }
}

/// Where the long words of `query` are in `text`, in bytes, to set them off. One shorter than
/// `WORD_START_MIN` only counts where it starts a word.
pub fn ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let chars: Vec<char> = text.chars().collect();
    let folded: Vec<char> = chars.iter().map(|&character| fold_char(character)).collect();
    let mut bytes = Vec::with_capacity(chars.len() + 1);
    let mut at = 0;
    for character in &chars {
        bytes.push(at);
        at += character.len_utf8();
    }
    bytes.push(at);
    let mut found: Vec<Range<usize>> = Vec::new();
    for word in tokens(query).long {
        let token = fold(&word);
        let needs_word_start = token.len() < WORD_START_MIN && starts_word_by_itself(token[0]);
        let mut from = 0;
        while let Some(index) = position(&folded, &token, from) {
            if needs_word_start && !starts_word(&chars, index) {
                from = index + 1;
                continue;
            }
            found.push(bytes[index]..bytes[index + token.len()]);
            from = index + token.len();
        }
    }
    found.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in found {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// A text folded as it is searched for, to find a phrase in as it is written but whatever its
/// case and accents. Folding keeps every character, so places in it are places in the text.
pub struct Folded(Vec<char>);

impl Folded {
    pub fn new(text: &str) -> Self {
        Self(fold(text))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Where `phrase` starts in it, in characters, each time it is there without overlapping.
    pub fn find(&self, phrase: &Folded) -> Vec<usize> {
        let mut starts = Vec::new();
        let mut from = 0;
        while let Some(index) = position(&self.0, &phrase.0, from) {
            starts.push(index);
            from = index + phrase.0.len();
        }
        starts
    }
}

/// The message on one line from a few words before `offset`, as a result's row shows it: without
/// its emphasis and code marks, with "…" where it was cut.
pub fn excerpt(text: &str, offset: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let at = text.get(..offset).map_or(0, |before| before.chars().count());
    let mut start = at.saturating_sub(EXCERPT_LEAD);
    if start > 0
        && let Some(space) = chars[start..at].iter().position(|character| character.is_whitespace())
    {
        start += space + 1;
    }
    let raw_end = chars.len().min(start + EXCERPT_LENGTH * 2);
    let raw: String = chars[start..raw_end].iter().collect();
    let unmarked = raw.replace("**", "").replace('`', "");
    let shown: Vec<char> = unmarked.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    let end = shown.len().min(EXCERPT_LENGTH - 2);
    let cut = end < shown.len() || raw_end < chars.len();
    let lead = if start > 0 { "…" } else { "" };
    let tail = if cut { "…" } else { "" };
    format!("{lead}{}{tail}", shown[..end].iter().collect::<String>().trim_end())
}

/// The message as it reads: a Markdown link as its text, which is all a thread shows of it.
pub fn link_text(text: &str) -> String {
    let mut read = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let label_end = rest[open..].find(']').map(|end| open + end);
        let link = label_end.and_then(|end| {
            let label = &rest[open + 1..end];
            let target = rest[end + 1..].strip_prefix('(')?;
            let close = target.find(')')?;
            let bare = !label.contains('\n') && !target[..close].contains(char::is_whitespace);
            bare.then_some((label, end + 2 + close + 1))
        });
        let Some((label, after)) = link else {
            read.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
            continue;
        };
        read.push_str(rest[..open].strip_suffix('!').unwrap_or(&rest[..open]));
        read.push_str(label);
        rest = &rest[after..];
    }
    read.push_str(rest);
    read
}

/// A letter as it is searched for: lowercase, without accents, a curly apostrophe straight. One
/// that would fold to more than one letter stays as it is, so places in the text stay put.
pub fn fold_char(character: char) -> char {
    if character.is_ascii() {
        return character.to_ascii_lowercase();
    }
    let mut folded = character.nfkd().filter(|mark| !is_combining_mark(*mark)).flat_map(char::to_lowercase);
    let single = match (folded.next(), folded.next()) {
        (Some(first), None) => first,
        _ => {
            let mut lower = character.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(first), None) => first,
                _ => character,
            }
        }
    };
    match single {
        'ς' => 'σ',
        '‘' | '’' => '\'',
        other => other,
    }
}

fn fold(text: &str) -> Vec<char> {
    text.chars().map(fold_char).collect()
}

fn position(text: &[char], token: &[char], from: usize) -> Option<usize> {
    if token.is_empty() || token.len() > text.len() {
        return None;
    }
    (from..=text.len() - token.len()).find(|&index| text[index..index + token.len()] == *token)
}

fn is_symbol(character: char) -> bool {
    !character.is_ascii()
        && !character.is_alphanumeric()
        && !character.is_whitespace()
        && !matches!(character, '\u{00A0}'..='\u{00BF}' | '\u{2000}'..='\u{206F}' | '\u{3000}'..='\u{303F}')
}

/// Chinese and Japanese have no spaces: each ideograph or kana starts a word.
fn is_cjk(character: char) -> bool {
    matches!(character as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn starts_word_by_itself(character: char) -> bool {
    character.is_alphanumeric() || is_symbol(character)
}

/// Whether a word starts at `index`: after what isn't a letter or a digit, at a camelCase or a
/// digit's edge, or at any Chinese or Japanese character, but not after an apostrophe in a word.
fn starts_word(text: &[char], index: usize) -> bool {
    let character = text[index];
    let previous = index.checked_sub(1).map(|at| text[at]);
    let before = index.checked_sub(2).map(|at| text[at]);
    let next = text.get(index + 1).copied();
    let after_apostrophe =
        matches!(previous, Some('\'' | '’')) && before.is_some_and(|before| before.is_alphanumeric());
    if !starts_word_by_itself(character) || after_apostrophe {
        return false;
    }
    let Some(previous) = previous else { return true };
    !previous.is_alphanumeric()
        || (previous.is_lowercase() && character.is_uppercase())
        || (previous.is_uppercase() && character.is_uppercase() && next.is_some_and(char::is_lowercase))
        || previous.is_numeric() != character.is_numeric()
        || is_cjk(character)
}

/// Whether a word of the search found at `index` starts a word. One that opens with punctuation,
/// like ".ts", brings its own edge.
fn starts_word_at(text: &[char], index: usize) -> bool {
    !starts_word_by_itself(text[index]) || starts_word(text, index)
}

fn ends_word_at(text: &[char], end: usize) -> bool {
    end >= text.len() || !text[end].is_alphanumeric() || starts_word_at(text, end)
}

/// A commit's hash or part of a UUID: seven or more hex digits, one of them a digit.
fn is_hex_id(run: &[char]) -> bool {
    run.len() >= 7 && run.iter().all(char::is_ascii_hexdigit) && run.iter().any(char::is_ascii_digit)
}

/// Every place `token` best sits in `folded`, a fold of `text`: as a whole word, else starting
/// one, else inside one. Inside a hex ID it only counts at its start ("cafe" in "7cafe52bb8").
fn find_token(text: &[char], folded: &[char], token: &[char]) -> Option<(Vec<usize>, f64)> {
    let mut quality = 0.0;
    let mut at = Vec::new();
    let mut run = (0, 0, false);
    let mut from = 0;
    while let Some(index) = position(folded, token, from) {
        from = index + 1;
        let end = index + token.len();
        if end > run.1 {
            let start = (0..index).rev().take_while(|&at| text[at].is_alphanumeric()).last().unwrap_or(index);
            let stop = (end..text.len()).find(|&at| !text[at].is_alphanumeric()).unwrap_or(text.len());
            run = (start, stop, is_hex_id(&text[start..stop]));
        }
        if run.2 && run.0 < index {
            continue;
        }
        let found = if !starts_word_at(text, index) {
            MID_WORD
        } else if ends_word_at(text, end) {
            WHOLE_WORD
        } else {
            PREFIX
        };
        if found > quality {
            quality = found;
            at = vec![index];
        } else if found == quality {
            at.push(index);
        }
    }
    (!at.is_empty()).then_some((at, quality))
}

/// Where the words are written side by side as whole words, as typed, whatever space is between
/// them. Only for a search of more than one word.
fn phrase(folded: &[char], words: &[Vec<char>]) -> Option<usize> {
    if words.len() < 2 {
        return None;
    }
    let edge = |at: usize| at == 0 || at >= folded.len() || !folded[at].is_alphanumeric();
    let mut from = 0;
    while let Some(start) = position(folded, &words[0], from) {
        from = start + 1;
        if start > 0 && !edge(start - 1) {
            continue;
        }
        let mut at = start + words[0].len();
        let together = words[1..].iter().all(|word| {
            let spaces = folded[at..].iter().take_while(|character| character.is_whitespace()).count();
            let fits = spaces > 0 && folded[at + spaces..].starts_with(word);
            at += spaces + word.len();
            fits
        });
        if together && edge(at) {
            return Some(start);
        }
    }
    None
}

/// The shortest stretch that holds a place of each word, from each word's places: where it
/// starts and how long it is.
fn shortest_span(places: &[Vec<usize>]) -> (usize, usize) {
    let mut hits: Vec<(usize, usize)> =
        places.iter().enumerate().flat_map(|(word, at)| at.iter().map(move |&index| (index, word))).collect();
    hits.sort_unstable();
    let mut counts = vec![0; places.len()];
    let mut covered = 0;
    let mut best = (hits.first().map_or(0, |hit| hit.0), usize::MAX);
    let mut left = 0;
    for &(index, word) in &hits {
        counts[word] += 1;
        if counts[word] == 1 {
            covered += 1;
        }
        while covered == places.len() {
            let (first, first_word) = hits[left];
            if index - first < best.1 {
                best = (first, index - first);
            }
            counts[first_word] -= 1;
            if counts[first_word] == 0 {
                covered -= 1;
            }
            left += 1;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grade(query: &str, text: &str) -> Option<f64> {
        Matcher::new(query).grade(text).map(|graded| graded.quality)
    }

    fn found<'a>(text: &'a str, query: &str) -> Vec<&'a str> {
        ranges(text, query).into_iter().map(|range| &text[range]).collect()
    }

    #[test]
    fn a_search_is_its_words_without_the_quotes_around_it_the_short_ones_apart() {
        let tokens = tokens("  “Fix the   Sidebar’s” ");
        assert_eq!(tokens.long, ["fix", "the", "sidebar's"]);
        assert!(tokens.short.is_empty());
        assert_eq!(super::tokens("go to nvme").short, ["go", "to"]);
        assert!(!searchable("go to"));
        assert!(searchable("go far"));
    }

    #[test]
    fn whole_words_grade_above_word_starts_above_the_inside_of_words() {
        assert_eq!(grade("deploy", "The DEPLOY failed"), Some(WHOLE_WORD));
        assert_eq!(grade("mode", "the model"), Some(PREFIX));
        assert_eq!(grade("odel", "the model"), Some(MID_WORD));
        assert_eq!(grade("fts", "two drafts"), None);
        assert_eq!(grade("cafe", "a commit 7cafe52bb8"), None);
        assert_eq!(grade("fooBar", "call fooBar now"), Some(WHOLE_WORD));
        assert_eq!(grade("bar", "call fooBar now"), Some(WHOLE_WORD));
    }

    #[test]
    fn words_match_in_any_order_whatever_their_case_and_accents() {
        assert_eq!(grade("green deploy", "The deploy is green"), Some(WHOLE_WORD + NEAR_BONUS));
        assert_eq!(grade("cafe", "Un Café"), Some(WHOLE_WORD));
        assert_eq!(grade("don't", "I don’t know"), Some(WHOLE_WORD));
        assert_eq!(grade("green deploy", "deploy"), None);
        assert_eq!(grade("deploy is", "deploy it"), None);
    }

    #[test]
    fn words_written_together_grade_highest_and_the_excerpt_starts_there() {
        let text = "the sidebar again, then fix it. Later: fix the   sidebar";
        let graded = Matcher::new("fix the sidebar").grade(text).unwrap();
        assert_eq!(graded.quality, WHOLE_WORD + PHRASE_BONUS);
        assert_eq!(&text[graded.offset..], "fix the   sidebar");
        let far = format!("deploy {} green", "word ".repeat(20));
        assert_eq!(grade("green deploy", &far), Some(WHOLE_WORD));
    }

    #[test]
    fn the_long_words_are_set_off_and_a_short_one_only_where_it_starts_a_word() {
        assert_eq!(found("Deploy it, then deploy é again", "deploy"), ["Deploy", "deploy"]);
        assert_eq!(found("Un café, cafe", "cafe"), ["café", "cafe"]);
        assert_eq!(found("two drafts, fts", "fts"), ["fts"]);
        assert_eq!(found("the deploy", "the deploy"), ["the", "deploy"]);
        assert!(found("deploy", "go").is_empty());
    }

    #[test]
    fn an_excerpt_is_one_line_from_a_word_a_little_before_the_match() {
        let text = "First line\n\nthen a much longer second line that goes on and on until the word needle shows up";
        assert_eq!(excerpt(text, text.find("needle").unwrap()), "…and on until the word needle shows up");
        assert_eq!(excerpt("Find the **needle** in `code`", 9), "Find the needle in code");
        let long = excerpt(&"word ".repeat(100), 0);
        assert!(long.ends_with('…') && long.chars().count() <= EXCERPT_LENGTH, "{long}");
    }

    #[test]
    fn a_link_reads_as_its_text() {
        assert_eq!(link_text("See [the docs](https://x.y/a) and ![shot](/tmp/a.png)."), "See the docs and shot.");
        assert_eq!(link_text("a [b] (c) [d](e f)"), "a [b] (c) [d](e f)");
    }
}
