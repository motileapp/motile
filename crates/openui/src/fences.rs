//! Where a reply's programs are, as the server and the clients both find them: each top-level
//! fence of `openui-lang` (or `openui`), and once a thread has a program, a later fence with no
//! `root` statement is a patch merged over the latest.

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag};

use crate::merge::merge;
use crate::parser::parse;

/// The languages of the fences that hold a program.
pub const LANGUAGES: [&str; 2] = ["openui-lang", "openui"];

/// Whether a fenced block of this info string holds a program.
pub fn holds_program(info: &str) -> bool {
    LANGUAGES.contains(&info.split_whitespace().next().unwrap_or_default())
}

/// The text of each program of a reply: its top-level program fences.
pub fn fences(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut depth = 0usize;
    let mut open: Option<String> = None;
    for event in Parser::new(text) {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if depth == 0 => {
                open = holds_program(&info).then(String::new);
                depth += 1;
            }
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    found.extend(open.take());
                }
            }
            Event::Text(text) => {
                if let Some(open) = &mut open {
                    open.push_str(&text);
                }
            }
            _ => {}
        }
    }
    found
}

/// A fence with no `root` statement, which patches the thread's latest program when it has one.
pub fn is_patch(program: &str) -> bool {
    !parse(program).statements.contains_key("root")
}

/// The program a fence shows after the thread's latest one, and whether it patched it.
pub fn over(latest: Option<&str>, program: &str) -> (String, bool) {
    match latest {
        Some(latest) if is_patch(program) => (merge(latest, program), true),
        _ => (program.to_string(), false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_top_level_program_fences_are_programs() {
        let text =
            "```openui\na = 1\n```\n\n- ```openui-lang\n  b = 2\n  ```\n\n```rust\nc\n```\n\n```openui-lang\nd = 3";
        assert_eq!(fences(text), ["a = 1\n", "d = 3"]);
    }

    #[test]
    fn a_fence_without_root_patches_the_latest_program() {
        let first = "root = Col([t])\nt = Text(\"a\")";
        assert_eq!(over(None, "t = Text(\"b\")"), ("t = Text(\"b\")".to_string(), false));
        assert_eq!(over(Some(first), "t = Text(\"b\")"), ("root = Col([t])\nt = Text(\"b\")".to_string(), true));
        assert_eq!(over(Some(first), "root = Col([])"), ("root = Col([])".to_string(), false));
    }
}
