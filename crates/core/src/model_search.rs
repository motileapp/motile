//! The composer's model picker search: every word typed is found in a model's name, its short
//! name, its account's name or its agent's, in any order.

/// The words of what is typed, in lower case.
pub fn words(query: &str) -> Vec<String> {
    query.to_lowercase().split(' ').filter(|word| !word.is_empty()).map(String::from).collect()
}

/// Whether every word starts a word of the texts, or is found in them when it is long enough to
/// mean it: "opus work", "work opus", "gpt-6".
pub fn matches(words: &[String], texts: &[&str]) -> bool {
    let text = texts.join(" ").to_lowercase();
    let parts: Vec<&str> =
        text.split(|c: char| !c.is_alphanumeric() && c != '.').filter(|part| !part.is_empty()).collect();
    words.iter().all(|word| {
        parts.iter().any(|part| part.starts_with(word.as_str()))
            || (word.chars().count() > 2 && text.contains(word.as_str()))
    })
}

/// Which of the rows match the query, for the clients that ask the core over FFI:
/// `{"query": "opus work", "rows": [["Opus 5.5", "Work", …], …]}` in, the matching rows' indices
/// out, as JSON.
pub fn matching_json(input: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Input {
        query: String,
        rows: Vec<Vec<String>>,
    }
    let Ok(input) = serde_json::from_str::<Input>(input) else { return "[]".into() };
    let words = words(&input.query);
    let found: Vec<usize> = input
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| matches(&words, &row.iter().map(String::as_str).collect::<Vec<_>>()))
        .map(|(index, _)| index)
        .collect();
    serde_json::to_string(&found).unwrap_or_else(|_| "[]".into())
}

#[cfg(test)]
mod tests {
    use super::{matches, matching_json, words};

    #[test]
    fn the_clients_get_the_indices_of_the_rows_that_match() {
        let input =
            r#"{"query": "work opus", "rows": [["Opus 5.5", "Default"], ["Opus 5.5", "Work"], ["Fable 5.1", "Work"]]}"#;
        assert_eq!(matching_json(input), "[1]");
        assert_eq!(matching_json("nonsense"), "[]");
    }

    fn found(query: &str) -> bool {
        matches(&words(query), &["Claude Opus 5.5", "Opus 5.5", "Work", "Claude Code"])
    }

    #[test]
    fn every_word_is_found_in_any_order() {
        assert!(found(""));
        assert!(found("opus work"));
        assert!(found("work opus"));
        assert!(found("WORK  Op"));
        assert!(found("5.5"));
        assert!(!found("opus personal"));
        assert!(!found("sonnet"));
    }

    #[test]
    fn a_short_word_only_starts_a_word_and_a_longer_one_is_found_anywhere() {
        assert!(!found("pu"));
        assert!(found("pus"));
        assert!(matches(&words("gpt-6"), &["GPT-6", "GPT-6", "Default", "Codex"]));
        assert!(matches(&words("6"), &["GPT-6", "GPT-6", "Default", "Codex"]));
    }
}
