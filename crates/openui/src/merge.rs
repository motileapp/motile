//! Incremental editing (language.md §7): a patch's statements merged into a program by name.

use std::collections::HashSet;

use indexmap::IndexMap;

use crate::ast::Expr;
use crate::lexer;
use crate::parser;
use crate::source;

struct Piece {
    text: String,
    expr: Expr,
}

/// The program with the patch merged in: a statement of the patch replaces the one of its name
/// or is added at the end, `name = null` deletes, and what `root` can't reach is removed, state
/// declarations aside. Both texts are read as the parser reads them.
pub fn merge(existing: &str, patch: &str) -> String {
    let mut merged: IndexMap<String, Piece> = IndexMap::new();
    for (name, piece) in pieces(existing) {
        merged.insert(name, piece);
    }
    for (name, piece) in pieces(patch) {
        if piece.expr == Expr::Null {
            merged.shift_remove(&name);
            continue;
        }
        merged.insert(name, piece);
    }
    if merged.contains_key("root") {
        let reachable: HashSet<String> = reachable(&merged).into_iter().map(str::to_owned).collect();
        merged.retain(|name, _| name.starts_with('$') || reachable.contains(name.as_str()));
    }
    merged.values().map(|piece| piece.text.as_str()).collect::<Vec<_>>().join("\n")
}

/// The statements of a text with their source, in order.
fn pieces(text: &str) -> Vec<(String, Piece)> {
    let prepared = source::prepare(text, false);
    let tokens = lexer::tokenize(&prepared);
    let (found, _) = parser::scan(&tokens, true);
    found
        .into_iter()
        .filter_map(|range| {
            let statement = parser::build_statement(&tokens[range.clone()])?;
            let text = prepared[tokens[range.start].start..tokens[range.end - 1].end].to_owned();
            Some((statement.name, Piece { text, expr: statement.expr }))
        })
        .collect()
}

/// The statements `root` reaches through references, the arguments of queries and mutations
/// included.
fn reachable(merged: &IndexMap<String, Piece>) -> HashSet<&str> {
    let mut reached: HashSet<&str> = HashSet::from(["root"]);
    let mut queue = vec!["root"];
    while let Some(name) = queue.pop() {
        let Some(piece) = merged.get(name) else { continue };
        for name in piece.expr.refs() {
            let Some((name, _)) = merged.get_key_value(name.as_str()) else { continue };
            if reached.insert(name) {
                queue.push(name);
            }
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_multiline_statements_whole() {
        let existing = "root = Stack([s])\ns = $n > 0\n  ? \"some\"\n  : \"none\"";
        assert_eq!(
            merge(existing, "s = $n > 1 ?\n  \"many\" :\n  \"few\""),
            "root = Stack([s])\ns = $n > 1 ?\n  \"many\" :\n  \"few\""
        );
        assert_eq!(merge(existing, "root = Stack([]) // gone"), "root = Stack([])");
    }

    #[test]
    fn keeps_what_queries_reference() {
        let existing = "root = Stack([t])\nt = Text(data.title)\ndata = Query(\"get\", {id: picked}, {})\npicked = 4\nold = Text(\"x\")";
        assert_eq!(
            merge(existing, "$v = 1"),
            "root = Stack([t])\nt = Text(data.title)\ndata = Query(\"get\", {id: picked}, {})\npicked = 4\n$v = 1"
        );
    }
}
