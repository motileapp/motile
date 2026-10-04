//! A patch as git writes it, read into files whose lines a client draws one under the other, and
//! a file's text as such lines. Highlighting comes separately, a list of spans for each line.

use motile_protocol::wire::Change;
use serde::Serialize;

use super::highlight::ByLine;
use super::rows::language_of;

/// What a line of a file's diff is.
pub const UNCHANGED: u8 = 0;
pub const ADDED: u8 = 1;
pub const REMOVED: u8 = 2;
/// Stands between two hunks, for the lines that are left out there, and says what git says
/// the next hunk is in: the function, the section.
pub const HUNK: u8 = 3;

#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct FileDiff {
    pub path: String,
    /// Where a renamed file was.
    pub from: Option<String>,
    pub change: Change,
    pub added: u32,
    pub removed: u32,
    /// Not text, so it has no lines.
    pub binary: bool,
    pub lines: Vec<String>,
    /// What each line is.
    pub kinds: Vec<u8>,
    /// Each line's number in the file as it was and as it is, 0 where it isn't in that one.
    pub old: Vec<u32>,
    pub new: Vec<u32>,
}

impl FileDiff {
    fn new(path: String) -> Self {
        Self {
            path,
            from: None,
            change: Change::Modified,
            added: 0,
            removed: 0,
            binary: false,
            lines: Vec::new(),
            kinds: Vec::new(),
            old: Vec::new(),
            new: Vec::new(),
        }
    }

    fn push(&mut self, kind: u8, old: u32, new: u32, text: &str) {
        self.lines.push(text.to_string());
        self.kinds.push(kind);
        self.old.push(old);
        self.new.push(new);
    }
}

/// The lines still to come in a hunk, and the numbers the next ones have.
#[derive(Default)]
struct Hunk<'a> {
    old: u32,
    new: u32,
    old_left: u32,
    new_left: u32,
    /// What the hunk is in, as git found it above the hunk.
    within: &'a str,
}

/// Reads the patch of `git diff`.
pub fn parse(patch: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut hunk = Hunk::default();
    for line in patch.lines() {
        if let Some(names) = line.strip_prefix("diff --git ") {
            files.push(FileDiff::new(named_twice(names)));
            hunk = Hunk::default();
            continue;
        }
        let Some(file) = files.last_mut() else { continue };
        if hunk.old_left == 0 && hunk.new_left == 0 {
            match heading(line) {
                Some(next) => {
                    // Nothing is left out above a hunk that starts the file.
                    if !file.lines.is_empty() || next.old > 1 || next.new > 1 {
                        file.push(HUNK, 0, 0, format!("⋯ {}", next.within).trim_end());
                    }
                    hunk = next;
                }
                None => describe(file, line),
            }
            continue;
        }
        let text = line.get(1..).unwrap_or_default();
        match line.as_bytes().first() {
            Some(b'+') => {
                file.push(ADDED, 0, hunk.new, text);
                file.added += 1;
                hunk.new += 1;
                hunk.new_left = hunk.new_left.saturating_sub(1);
            }
            Some(b'-') => {
                file.push(REMOVED, hunk.old, 0, text);
                file.removed += 1;
                hunk.old += 1;
                hunk.old_left = hunk.old_left.saturating_sub(1);
            }
            // The file ends without a line break, which git says on a line of its own.
            Some(b'\\') => {}
            _ => {
                file.push(UNCHANGED, hunk.old, hunk.new, text);
                hunk.old += 1;
                hunk.new += 1;
                hunk.old_left = hunk.old_left.saturating_sub(1);
                hunk.new_left = hunk.new_left.saturating_sub(1);
            }
        }
    }
    files
}

/// Reads `@@ -12,7 +12,8 @@`, the heading of a hunk.
fn heading(line: &str) -> Option<Hunk<'_>> {
    let (ranges, within) = line.strip_prefix("@@ -")?.split_once(" @@")?;
    let mut parts = ranges.split(' ');
    let range = |part: &str| {
        let (start, count) = part.split_once(',').unwrap_or((part, "1"));
        Some((start.parse().ok()?, count.parse().ok()?))
    };
    let (old, old_left) = range(parts.next()?)?;
    let (new, new_left) = range(parts.next()?.strip_prefix('+')?)?;
    Some(Hunk { old, new, old_left, new_left, within: within.trim() })
}

/// Takes in a line that comes before a file's hunks and says something about the file.
fn describe(file: &mut FileDiff, line: &str) {
    let name = |name: &str| unquoted(name.trim_end_matches('\t'));
    if line.starts_with("new file mode") {
        file.change = Change::Added;
    } else if line.starts_with("deleted file mode") {
        file.change = Change::Deleted;
    } else if let Some(from) = line.strip_prefix("rename from ") {
        file.change = Change::Renamed;
        file.from = Some(name(from));
    } else if let Some(to) = line.strip_prefix("rename to ") {
        file.path = name(to);
    } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
        file.binary = true;
    } else if let Some(path) =
        line.strip_prefix("+++ ").map(name).and_then(|path| path.strip_prefix("b/").map(String::from))
    {
        file.path = path;
    }
}

/// The path in `a/path b/path`, which is what git calls a file that wasn't renamed.
fn named_twice(names: &str) -> String {
    if names.starts_with('"') {
        let first = names.split("\" ").next().unwrap_or(names);
        return unquoted(&format!("{first}\"")).strip_prefix("a/").map(String::from).unwrap_or_default();
    }
    let half = names.len().saturating_sub(1) / 2;
    names.get(2..half).unwrap_or(names).to_string()
}

/// Git puts a path with unusual characters in quotes, with backslashes before some of them.
fn unquoted(path: &str) -> String {
    let Some(inner) = path.strip_prefix('"').and_then(|path| path.strip_suffix('"')) else { return path.to_string() };
    let mut plain = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            plain.push(character);
            continue;
        }
        match characters.next() {
            Some('t') => plain.push('\t'),
            Some('n') => plain.push('\n'),
            Some(other) => plain.push(other),
            None => {}
        }
    }
    plain
}

/// The highlighting of each line of the file's diff. The lines that were removed are read as
/// the file was, the others as it is, and every hunk starts anew.
pub fn highlight(file: &FileDiff) -> Vec<Vec<u32>> {
    let language = language_of(&file.path);
    let (mut was, mut is) = (ByLine::new(&language), ByLine::new(&language));
    let mut spans = Vec::with_capacity(file.lines.len());
    for (line, kind) in file.lines.iter().zip(&file.kinds) {
        spans.push(match *kind {
            HUNK => {
                (was, is) = (ByLine::new(&language), ByLine::new(&language));
                Vec::new()
            }
            REMOVED => was.line(line),
            ADDED => is.line(line),
            _ => {
                was.line(line);
                is.line(line)
            }
        });
    }
    spans
}

/// The lines of a file's text. Of a text that was cut, the last line is only a part and is left out.
pub fn lines_of(text: &str, cut: bool) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if cut {
        lines.pop();
    }
    lines
}

/// The highlighting of each line of the file at `path`.
pub fn highlight_lines(path: &str, lines: &[String]) -> Vec<Vec<u32>> {
    let mut highlighter = ByLine::new(&language_of(path));
    lines.iter().map(|line| highlighter.line(line)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/src/greet.rs b/src/greet.rs
index 1111111..2222222 100644
--- a/src/greet.rs
+++ b/src/greet.rs
@@ -1,4 +1,5 @@
 // Greets.
-fn greet() -> &'static str {
-    \"hello\"
+fn greet(name: &str) -> String {
+    // By name.
+    format!(\"hello {name}\")
 }
@@ -20,2 +21,2 @@ fn main() {
-    old();
+    new();
 }
\\ No newline at end of file
diff --git a/notes with space.md b/notes with space.md
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/notes with space.md\t
@@ -0,0 +1 @@
+later
diff --git a/old name.rs b/new name.rs
similarity index 100%
rename from old name.rs
rename to new name.rs
diff --git a/logo.png b/logo.png
index 4444444..5555555 100644
Binary files a/logo.png and b/logo.png differ
diff --git a/gone.txt b/gone.txt
deleted file mode 100644
index 6666666..0000000
--- a/gone.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-one
---- two
";

    #[test]
    fn a_patch_is_read_into_files_with_numbered_lines() {
        let files = parse(PATCH);
        let seen: Vec<_> =
            files.iter().map(|file| (file.path.as_str(), file.change, file.added, file.removed)).collect();
        assert_eq!(
            seen,
            [
                ("src/greet.rs", Change::Modified, 4, 3),
                ("notes with space.md", Change::Added, 1, 0),
                ("new name.rs", Change::Renamed, 0, 0),
                ("logo.png", Change::Modified, 0, 0),
                ("gone.txt", Change::Deleted, 0, 2),
            ]
        );
        assert_eq!(files[2].from.as_deref(), Some("old name.rs"));
        assert!(files[3].binary && files[3].lines.is_empty());

        let greet = &files[0];
        assert_eq!(
            greet.kinds,
            [UNCHANGED, REMOVED, REMOVED, ADDED, ADDED, ADDED, UNCHANGED, HUNK, REMOVED, ADDED, UNCHANGED]
        );
        assert_eq!(greet.old, [1, 2, 3, 0, 0, 0, 4, 0, 20, 0, 21]);
        assert_eq!(greet.new, [1, 0, 0, 2, 3, 4, 5, 0, 0, 21, 22]);
        assert_eq!(greet.lines[3], "fn greet(name: &str) -> String {");
        assert_eq!(greet.lines[7], "⋯ fn main() {");
        // A removed line that looks like a file's heading is still a line.
        assert_eq!(files[4].lines, ["one", "--- two"]);
    }

    #[test]
    fn a_path_git_put_in_quotes_is_read_plain() {
        let files = parse("diff --git \"a/tab\\there.txt\" \"b/tab\\there.txt\"\nnew file mode 100644\n");
        assert_eq!(files[0].path, "tab\there.txt");
        assert_eq!(unquoted("\"say \\\"hi\\\".txt\""), "say \"hi\".txt");
    }

    #[test]
    fn every_line_of_a_diff_gets_its_own_spans() {
        let files = parse(PATCH);
        let spans = highlight(&files[0]);
        assert_eq!(spans.len(), files[0].lines.len());
        assert!(spans[7].is_empty(), "what stands between two hunks has no colours");
        // `fn` is a keyword at the start of the line that was removed and of the one that was added.
        assert_eq!(spans[1][..2], [0, 2]);
        assert_eq!(spans[3][..2], [0, 2]);
        let within =
            |line: usize| spans[line].chunks(3).all(|span| span[0] + span[1] <= files[0].lines[line].len() as u32);
        assert!((0..spans.len()).all(within), "no span reaches past its line");
    }

    #[test]
    fn a_text_that_was_cut_loses_its_last_partial_line() {
        assert_eq!(lines_of("one\ntwo\nthr", true), ["one", "two"]);
        assert_eq!(lines_of("one\r\ntwo\n", false), ["one", "two"]);
        let spans = highlight_lines("a.rs", &lines_of("fn a() {}\n// done", false));
        assert_eq!((spans.len(), spans[0][..2].to_vec()), (2, vec![0, 2]));
    }
}
