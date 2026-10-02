//! Streamed text is passed on in finished blocks rather than token by token, so a reply grows by
//! paragraphs instead of flickering through every word.

use std::time::Duration;

/// Blocks that finish sooner than this after the last ones wait and go together.
pub const DELIVER_EVERY: Duration = Duration::from_millis(400);

/// How much of `text`, from its start, is finished: paragraphs closed by a blank line, list
/// items and what precedes a heading or a code block, and inside a code block every whole line.
pub fn settled_len(text: &str) -> usize {
    let mut settled = 0;
    let mut fence: Option<(char, usize)> = None;
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        if !line.ends_with('\n') {
            break;
        }
        let end = start + line.len();
        let content = line.trim();
        match fence {
            Some(opened) => {
                settled = end;
                if closes(content, opened) {
                    fence = None;
                }
            }
            None if content.is_empty() => settled = end,
            None => {
                fence = fence_marker(content);
                if fence.is_some() || starts_block(content) {
                    settled = start;
                }
            }
        }
        start = end;
    }
    settled
}

fn fence_marker(content: &str) -> Option<(char, usize)> {
    let marker = content.chars().next().filter(|first| matches!(first, '`' | '~'))?;
    let length = content.chars().take_while(|character| *character == marker).count();
    (length >= 3).then_some((marker, length))
}

fn closes(content: &str, (marker, length): (char, usize)) -> bool {
    content.chars().all(|character| character == marker) && content.chars().count() >= length
}

/// Whether the line starts a list item or a heading, which ends whatever came before it.
fn starts_block(content: &str) -> bool {
    let after_hashes = content.trim_start_matches('#');
    if after_hashes.len() < content.len() && content.len() - after_hashes.len() <= 6 && after_hashes.starts_with(' ') {
        return true;
    }
    if ["- ", "* ", "+ "].iter().any(|bullet| content.starts_with(bullet)) {
        return true;
    }
    let after_digits = content.trim_start_matches(|character: char| character.is_ascii_digit());
    after_digits.len() < content.len() && (after_digits.starts_with(". ") || after_digits.starts_with(") "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(text: &str) -> &str {
        &text[..settled_len(text)]
    }

    #[test]
    fn a_paragraph_is_finished_by_a_blank_line() {
        assert_eq!(settled("One line"), "");
        assert_eq!(settled("One line\nand another\n"), "");
        assert_eq!(settled("One line\nand another\n\nNext"), "One line\nand another\n\n");
    }

    #[test]
    fn a_list_grows_by_items_and_a_heading_waits_for_what_follows() {
        assert_eq!(settled("- one\n- two\n- thr"), "- one\n");
        assert_eq!(settled("1. one\n2. two\n3. three\n"), "1. one\n2. two\n");
        assert_eq!(settled("Intro\n## Title\n"), "Intro\n");
        assert_eq!(settled("## Title\n\nBody"), "## Title\n\n");
    }

    #[test]
    fn a_code_block_grows_by_whole_lines() {
        assert_eq!(settled("Text\n```rust\n"), "Text\n");
        assert_eq!(settled("Text\n```rust\nlet a = 1;\nlet b"), "Text\n```rust\nlet a = 1;\n");
        // A blank line or a list marker inside the code is only code.
        assert_eq!(settled("```\n- a\n\n# b\nc"), "```\n- a\n\n# b\n");
        assert_eq!(settled("```\ncode\n```\nAfter"), "```\ncode\n```\n");
        assert_eq!(settled("````\n```\nstill code\n"), "````\n```\nstill code\n");
    }
}
