//! How a message names the skills it starts: `$name`, at the start of a word. The server finds
//! them to have the agent run them, and the clients to show them.

use std::ops::Range;

/// The `$name`s in `text`, each with its byte range, the `$` included. A name has a letter, and
/// `$20` or `$1e6` is money, not a skill.
pub fn mentions(text: &str) -> Vec<(Range<usize>, &str)> {
    let mut found = Vec::new();
    let mut previous = None;
    for (start, character) in text.char_indices() {
        let at_word_start = previous.is_none_or(char::is_whitespace);
        previous = Some(character);
        if character != '$' || !at_word_start {
            continue;
        }
        let rest = &text[start + 1..];
        let length = rest.find(|character| !is_name_character(character)).unwrap_or(rest.len());
        let name = rest[..length].trim_end_matches([':', '-']);
        if is_name(name) {
            found.push((start..start + 1 + name.len(), name));
        }
    }
    found
}

/// Whether `name` can follow a `$`.
pub fn is_name(name: &str) -> bool {
    let starts_with_digit = name.starts_with(|character: char| character.is_ascii_digit());
    name.chars().all(is_name_character) && name.chars().any(|character| character.is_alphabetic()) && !starts_with_digit
}

pub fn is_name_character(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '-' | '_' | ':')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<&str> {
        mentions(text).into_iter().map(|(_, name)| name).collect()
    }

    #[test]
    fn a_mention_starts_a_word_and_ends_where_a_name_cannot_go_on() {
        assert_eq!(names("$ship-it now, then $review: and $plugin:docs."), ["ship-it", "review", "plugin:docs"]);
        assert_eq!(names("costs $20 or $1e6, a$b, $-, $"), Vec::<&str>::new());
        assert_eq!(mentions("go $ship-it")[0].0, 3..11);
    }
}
