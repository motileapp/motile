//! Statements and expressions (language.md §1.4, §1.5) and the streaming parser (§4).
//!
//! One scan finds where statements end, for the batch parse, the streaming parse and the edit
//! merge alike. While streaming, a statement is complete once its ending newline has arrived and
//! nothing can extend it; completed statements are parsed once and kept, and the text after them
//! is parsed again on every chunk after implicit closing.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

use indexmap::IndexMap;

use crate::ast::{BinaryOp, Expr, Statement, UnaryOp};
use crate::builtins::Builtin;
use crate::js;
use crate::lexer::{self, Tok, Token};
use crate::source;

/// What a parse found: the statements by name, a later definition in place of an earlier one.
#[derive(Debug, Clone, Default)]
pub struct Parsed {
    pub statements: IndexMap<String, Arc<Statement>>,
    /// The names the pending tail defines: still streaming.
    pub pending: Vec<String>,
    /// The text ended inside an unclosed string or bracket.
    pub incomplete: bool,
    /// Every statement read, the ones a later definition replaced too.
    pub count: usize,
    /// A `$name` at the very end of the streamed text, which may still grow (`$d` of `$dishes`).
    pub growing: Option<String>,
    /// The stream has ended.
    pub complete: bool,
    /// The prepared text is not empty.
    pub has_text: bool,
}

impl Parsed {
    pub fn statement(&self, name: &str) -> Option<&Statement> {
        self.statements.get(name).map(|statement| statement.as_ref())
    }

    pub fn is_pending(&self, name: &str) -> bool {
        self.pending.iter().any(|pending| pending == name)
    }
}

/// Parses a whole text at once.
pub fn parse(text: &str) -> Parsed {
    Parser::new().push(text, true)
}

/// The streaming parser: feed it chunks, or the whole text so far.
#[derive(Debug, Clone, Default)]
pub struct Parser {
    buffer: String,
    prepared: String,
    /// How much of `prepared` holds completed statements.
    watermark: usize,
    completed: IndexMap<String, Arc<Statement>>,
    completed_count: usize,
    finished: bool,
    last: Option<Parsed>,
}

impl Parser {
    pub fn new() -> Parser {
        Parser::default()
    }

    /// Adds a chunk; `complete` once the stream has ended. A chunk after the end starts a new
    /// stream.
    pub fn push(&mut self, chunk: &str, complete: bool) -> Parsed {
        if self.finished {
            self.reset();
        }
        self.buffer.push_str(chunk);
        self.update(complete)
    }

    /// Sets the whole text so far. Text that does not extend the previous one starts again.
    pub fn set(&mut self, text: &str, complete: bool) -> Parsed {
        if self.finished
            && complete
            && text == self.buffer
            && let Some(last) = &self.last
        {
            return last.clone();
        }
        if self.finished || !text.starts_with(self.buffer.as_str()) {
            self.reset();
        }
        self.buffer.clear();
        self.buffer.push_str(text);
        self.update(complete)
    }

    /// The text received so far.
    pub fn text(&self) -> &str {
        &self.buffer
    }

    fn reset(&mut self) {
        *self = Parser::default();
    }

    fn update(&mut self, complete: bool) -> Parsed {
        let prepared = source::prepare(&self.buffer, !complete);
        if !prepared.starts_with(&self.prepared[..self.watermark]) {
            self.completed.clear();
            self.completed_count = 0;
            self.watermark = 0;
        }
        self.prepared = prepared;

        let tail = &self.prepared[self.watermark..];
        let tokens = lexer::tokenize(tail);
        let (found, pending) = scan(&tokens, false);
        for range in found {
            let Some(statement) = build_statement(&tokens[range]) else { continue };
            self.completed_count += 1;
            self.completed.insert(statement.name.clone(), Arc::new(statement));
        }
        let consumed = match pending {
            Some(index) => tokens[index].start,
            None => tokens.last().map_or(0, |token| token.end),
        };
        self.watermark += consumed;

        let pending_text = js::trim(&self.prepared[self.watermark..]);
        let (closed, incomplete) = auto_close(pending_text);
        let tokens = lexer::tokenize(&closed);
        let (found, _) = scan(&tokens, true);
        let mut statements = self.completed.clone();
        let mut pending_names = Vec::new();
        let mut count = self.completed_count;
        for range in found {
            let Some(statement) = build_statement(&tokens[range]) else { continue };
            count += 1;
            if !complete && incomplete && self.completed.contains_key(&statement.name) {
                continue;
            }
            pending_names.push(statement.name.clone());
            statements.insert(statement.name.clone(), Arc::new(statement));
        }
        let growing = if complete { None } else { growing_state(&self.buffer, pending_text) };
        self.finished = complete;
        let parsed = Parsed {
            statements,
            pending: pending_names,
            incomplete,
            count,
            growing,
            complete,
            has_text: !self.prepared.is_empty(),
        };
        if complete {
            self.last = Some(parsed.clone());
        }
        parsed
    }
}

/// A `$name` that ends the raw text and appears nowhere else in the pending text.
fn growing_state(buffer: &str, pending: &str) -> Option<String> {
    let bytes = buffer.as_bytes();
    let mut start = bytes.len();
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    if start == 0 || start == bytes.len() || bytes[start - 1] != b'$' {
        return None;
    }
    let name = &buffer[start - 1..];
    let before = pending.strip_suffix(name)?;
    let elsewhere = before.match_indices(name).any(|(at, _)| {
        !before.as_bytes().get(at + name.len()).is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    });
    (!elsewhere).then(|| name.to_owned())
}

/// Closes an unterminated string (adding a `\` first when it ends mid-escape), then the open
/// brackets in reverse order. A closing bracket that does not match is skipped. Whether anything
/// was closed comes with it.
pub fn auto_close(input: &str) -> (Cow<'_, str>, bool) {
    let mut stack: Vec<u8> = Vec::new();
    let mut quote: Option<u8> = None;
    let mut escape = false;
    for byte in input.bytes() {
        if escape {
            escape = false;
            continue;
        }
        if let Some(open) = quote {
            if byte == b'\\' {
                escape = true;
            } else if byte == open {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'(' | b'[' | b'{' => stack.push(byte),
            b')' | b']' | b'}' if stack.last().is_some_and(|open| closer(*open) == byte) => {
                stack.pop();
            }
            _ => {}
        }
    }
    if quote.is_none() && stack.is_empty() {
        return (Cow::Borrowed(input), false);
    }
    let mut out = input.to_owned();
    if let Some(open) = quote {
        if escape {
            out.push('\\');
        }
        out.push(open as char);
    }
    out.extend(stack.iter().rev().map(|open| closer(*open) as char));
    (Cow::Owned(out), true)
}

fn closer(open: u8) -> u8 {
    match open {
        b'(' => b')',
        b'[' => b']',
        _ => b'}',
    }
}

/// The token ranges of the statements in `tokens`, from the name to the end of the expression,
/// and the index where an unfinished statement starts. With `complete`, the text ends here.
pub fn scan(tokens: &[Token], complete: bool) -> (Vec<Range<usize>>, Option<usize>) {
    let mut found = Vec::new();
    let mut at = 0;
    loop {
        while tokens.get(at).is_some_and(|token| token.tok == Tok::Newline) {
            at += 1;
        }
        if at >= tokens.len() {
            return (found, None);
        }
        let start = at;
        let named = matches!(tokens[at].tok, Tok::Ident(_) | Tok::Type(_) | Tok::State(_));
        let shaped = named && tokens.get(at + 1).is_some_and(|token| token.tok == Tok::Equals);
        if !shaped {
            match tokens[at..].iter().position(|token| token.tok == Tok::Newline) {
                Some(offset) => at += offset,
                None if complete => return (found, None),
                None => return (found, Some(start)),
            }
            continue;
        }
        match statement_end(tokens, at + 2, complete) {
            Some(end) => {
                found.push(start..end);
                at = end;
            }
            None => return (found, Some(start)),
        }
    }
}

/// Where the expression from `from` ends: at a newline outside brackets and outside an open
/// ternary, unless the next line starts with `?`. None while that can't be told yet.
fn statement_end(tokens: &[Token], from: usize, complete: bool) -> Option<usize> {
    let mut depth = 0usize;
    let mut open = 0usize;
    let mut else_pending = false;
    let mut at = from;
    while at < tokens.len() {
        match tokens[at].tok {
            Tok::Newline if depth == 0 => {
                if open > 0 || else_pending {
                    at += 1;
                    continue;
                }
                let mut next = at + 1;
                while tokens.get(next).is_some_and(|token| token.tok == Tok::Newline) {
                    next += 1;
                }
                let Some(token) = tokens.get(next) else { return complete.then_some(at) };
                if token.tok != Tok::Question {
                    return Some(at);
                }
                at = next;
                continue;
            }
            Tok::Newline => {}
            Tok::LParen | Tok::LBrack | Tok::LBrace => {
                if depth == 0 {
                    else_pending = false;
                }
                depth += 1;
            }
            Tok::RParen | Tok::RBrack | Tok::RBrace => depth = depth.saturating_sub(1),
            Tok::Question if depth == 0 => open += 1,
            Tok::Colon if depth == 0 && open > 0 => {
                open -= 1;
                else_pending = true;
            }
            Tok::Not | Tok::Minus => {}
            _ if depth == 0 => else_pending = false,
            _ => {}
        }
        at += 1;
    }
    complete.then_some(tokens.len())
}

/// `name = expression` from a statement's tokens. None when the right side is empty.
pub fn build_statement(tokens: &[Token]) -> Option<Statement> {
    let (name, state) = match &tokens.first()?.tok {
        Tok::Ident(name) | Tok::Type(name) => (name.clone(), false),
        Tok::State(name) => (name.clone(), true),
        _ => return None,
    };
    let rhs: Vec<&Tok> = tokens.get(2..)?.iter().map(|token| &token.tok).filter(|tok| **tok != Tok::Newline).collect();
    if rhs.is_empty() {
        return None;
    }
    Some(Statement::new(name, state, parse_expression(&rhs)))
}

/// Parses an expression from its tokens, newlines left out. Tokens after a complete expression
/// are ignored.
pub fn parse_expression(tokens: &[&Tok]) -> Expr {
    ExprParser { tokens, at: 0 }.expr(0)
}

const TERNARY: u8 = 1;
const UNARY: u8 = 8;
const POSTFIX: u8 = 9;

struct ExprParser<'a> {
    tokens: &'a [&'a Tok],
    at: usize,
}

impl<'a> ExprParser<'a> {
    fn current(&self) -> Option<&'a Tok> {
        self.tokens.get(self.at).copied()
    }

    fn advance(&mut self) -> Option<&'a Tok> {
        let token = self.current();
        self.at += 1;
        token
    }

    fn eat(&mut self, tok: &Tok) {
        if self.current() == Some(tok) {
            self.at += 1;
        }
    }

    fn expr(&mut self, min: u8) -> Expr {
        let mut left = self.prefix();
        while self.current().map_or(0, infix_precedence) > min {
            left = self.infix(left);
        }
        left
    }

    fn prefix(&mut self) -> Expr {
        let Some(token) = self.advance() else { return Expr::Null };
        match token {
            Tok::Str(text) => Expr::Str(text.clone()),
            Tok::Num(value) => Expr::Num(*value),
            Tok::True => Expr::Bool(true),
            Tok::False => Expr::Bool(false),
            Tok::Null => Expr::Null,
            Tok::LBrack => Expr::Array(self.list(&Tok::RBrack)),
            Tok::LBrace => self.object(),
            Tok::State(name) if self.current() == Some(&Tok::Equals) => {
                self.at += 1;
                Expr::Assign(name.clone(), Box::new(self.expr(0)))
            }
            Tok::State(name) => Expr::State(name.clone()),
            Tok::Type(name) if self.current() == Some(&Tok::LParen) && callable(name) => self.call(name, false),
            Tok::At(name) if self.current() == Some(&Tok::LParen) => self.call(name, true),
            Tok::Type(name) | Tok::At(name) | Tok::Ident(name) => Expr::Ref(name.clone()),
            Tok::Not => Expr::Unary(UnaryOp::Not, Box::new(self.expr(UNARY))),
            Tok::Minus => Expr::Unary(UnaryOp::Neg, Box::new(self.expr(UNARY))),
            Tok::LParen => {
                let inner = self.expr(0);
                self.eat(&Tok::RParen);
                inner
            }
            _ => Expr::Null,
        }
    }

    fn infix(&mut self, left: Expr) -> Expr {
        let Some(token) = self.advance() else { return left };
        let op = match token {
            Tok::Question => {
                let then = self.expr(0);
                self.eat(&Tok::Colon);
                let otherwise = self.expr(0);
                return Expr::Ternary(Box::new(left), Box::new(then), Box::new(otherwise));
            }
            Tok::Dot => {
                let field = self.advance().map_or_else(|| "?".to_owned(), key_name);
                return Expr::Member(Box::new(left), field);
            }
            Tok::LBrack => {
                let index = self.expr(0);
                self.eat(&Tok::RBrack);
                return Expr::Index(Box::new(left), Box::new(index));
            }
            Tok::Or => BinaryOp::Or,
            Tok::And => BinaryOp::And,
            Tok::EqEq => BinaryOp::Eq,
            Tok::NotEq => BinaryOp::NotEq,
            Tok::Greater => BinaryOp::Greater,
            Tok::Less => BinaryOp::Less,
            Tok::GreaterEq => BinaryOp::GreaterEq,
            Tok::LessEq => BinaryOp::LessEq,
            Tok::Plus => BinaryOp::Add,
            Tok::Minus => BinaryOp::Sub,
            Tok::Star => BinaryOp::Mul,
            Tok::Slash => BinaryOp::Div,
            Tok::Percent => BinaryOp::Rem,
            _ => return left,
        };
        let right = self.expr(op.precedence());
        Expr::Binary(op, Box::new(left), Box::new(right))
    }

    fn call(&mut self, name: &str, at: bool) -> Expr {
        self.at += 1;
        let args = self.list(&Tok::RParen);
        Expr::Call { name: name.to_owned(), args, at }
    }

    /// Expressions up to `close`, commas optional.
    fn list(&mut self, close: &Tok) -> Vec<Expr> {
        let mut items = Vec::new();
        while self.current().is_some_and(|token| token != close) {
            items.push(self.expr(0));
            self.eat(&Tok::Comma);
        }
        self.eat(close);
        items
    }

    fn object(&mut self) -> Expr {
        let mut entries = Vec::new();
        while self.current().is_some_and(|token| *token != Tok::RBrace) {
            let key = self.advance().map_or_else(|| "?".to_owned(), key_name);
            self.eat(&Tok::Colon);
            entries.push((key, self.expr(0)));
            self.eat(&Tok::Comma);
        }
        self.eat(&Tok::RBrace);
        Expr::Object(entries)
    }
}

/// Built-ins need the `@`: `Count(x)` is the reference `Count`. Only `Action` works without.
fn callable(name: &str) -> bool {
    name == "Action" || Builtin::from_name(name).is_none()
}

/// An object key or a member's field: a name, a string, a number or a `$name` without its `$`;
/// any other token reads as `?`.
fn key_name(token: &Tok) -> String {
    match token {
        Tok::Ident(name) | Tok::Type(name) | Tok::Str(name) => name.clone(),
        Tok::Num(value) => js::number_to_string(*value),
        Tok::State(name) => name[1..].to_owned(),
        _ => "?".to_owned(),
    }
}

fn infix_precedence(token: &Tok) -> u8 {
    match token {
        Tok::Question => TERNARY,
        Tok::Or => BinaryOp::Or.precedence(),
        Tok::And => BinaryOp::And.precedence(),
        Tok::EqEq | Tok::NotEq => BinaryOp::Eq.precedence(),
        Tok::Greater | Tok::Less | Tok::GreaterEq | Tok::LessEq => BinaryOp::Greater.precedence(),
        Tok::Plus | Tok::Minus => BinaryOp::Add.precedence(),
        Tok::Star | Tok::Slash | Tok::Percent => BinaryOp::Mul.precedence(),
        Tok::Dot | Tok::LBrack => POSTFIX,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinaryOp, Expr, Kind};

    fn names(parsed: &Parsed) -> Vec<&str> {
        parsed.statements.keys().map(String::as_str).collect()
    }

    fn expr<'a>(parsed: &'a Parsed, name: &str) -> &'a Expr {
        &parsed.statement(name).unwrap().expr
    }

    #[test]
    fn keeps_a_ternary_open_across_lines() {
        let ternary = |then: &str, otherwise: &str| {
            Expr::Ternary(
                Box::new(Expr::Ref("x".into())),
                Box::new(Expr::Num(then.parse().unwrap())),
                Box::new(Expr::Num(otherwise.parse().unwrap())),
            )
        };
        for text in [
            "a = x ? 1 :\n  2\nb = 3",
            "a = x ?\n  1 : 2\nb = 3",
            "a = x\n  ? 1\n  : 2\nb = 3",
            "a = x ? 1\n\n : 2\nb = 3",
        ] {
            let parsed = parse(text);
            assert_eq!(names(&parsed), ["a", "b"], "{text:?}");
            assert_eq!(expr(&parsed, "a"), &ternary("1", "2"), "{text:?}");
        }
        let parsed = parse("a = x ? 1 : 2\n  + 3\nb = 3");
        assert_eq!(expr(&parsed, "a"), &ternary("1", "2"));
        assert_eq!(names(&parsed), ["a", "b"]);
        let parsed = parse("a = x ? 1 : -\n  2");
        assert!(matches!(expr(&parsed, "a"), Expr::Ternary(..)));
    }

    #[test]
    fn reads_precedence_and_skips_what_is_not_a_statement() {
        let parsed = parse("Some prose: (with a paren\nroot = 1 + 2 * 3 == 7 && !false\n= 4\nx");
        assert_eq!(names(&parsed), ["root"]);
        let Expr::Binary(BinaryOp::And, left, _) = expr(&parsed, "root") else { panic!("not &&") };
        assert!(matches!(**left, Expr::Binary(BinaryOp::Eq, ..)));
        let parsed = parse("a = Count(x)\nb = @Count(x)\nc = Upper(1)\nd = $v = 3");
        assert_eq!(expr(&parsed, "a"), &Expr::Ref("Count".into()));
        assert!(matches!(expr(&parsed, "b"), Expr::Call { at: true, .. }));
        assert!(matches!(expr(&parsed, "c"), Expr::Call { at: false, .. }));
        assert!(matches!(expr(&parsed, "d"), Expr::Assign(..)));
        let parsed = parse("$q = Query(\"t\", {}, {})\n$s = 1");
        assert_eq!(parsed.statement("$q").unwrap().kind, Kind::Query);
        assert_eq!(parsed.statement("$s").unwrap().kind, Kind::State);
    }

    #[test]
    fn closes_a_pending_tail() {
        assert_eq!(auto_close("a = F([1, \"x"), (Cow::Owned("a = F([1, \"x\"])".into()), true));
        assert_eq!(auto_close("a = \"x\\"), (Cow::Owned("a = \"x\\\\\"".into()), true));
        assert_eq!(auto_close("a = F(]"), (Cow::Owned("a = F(])".into()), true));
        assert_eq!(auto_close("a = 'it\\'s'"), (Cow::Borrowed("a = 'it\\'s'"), false));
    }

    #[test]
    fn never_lets_a_pending_statement_replace_a_completed_one() {
        let mut parser = Parser::new();
        let parsed = parser.push("t = Text(\"first\")\nt = Text(\"sec", false);
        assert_eq!(
            expr(&parsed, "t"),
            &Expr::Call { name: "Text".into(), args: vec![Expr::Str("first".into())], at: false }
        );
        assert!(parsed.incomplete);
        let parsed = parser.push("ond\")", false);
        assert_eq!(
            expr(&parsed, "t"),
            &Expr::Call { name: "Text".into(), args: vec![Expr::Str("second".into())], at: false }
        );
        let parsed = parser.push("\nu = Text(\"x", true);
        assert_eq!(names(&parsed), ["t", "u"]);
        assert!(parsed.complete && parsed.incomplete);
    }

    #[test]
    fn parses_a_completed_statement_once() {
        let mut parser = Parser::new();
        let first = parser.push("a = Text(\"a\")\nb = Te", false);
        let second = parser.push("xt(\"b\")\nc = 1", false);
        assert!(Arc::ptr_eq(&first.statements["a"], &second.statements["a"]));
        assert_eq!(parser.watermark, "a = Text(\"a\")\nb = Text(\"b\")\n".len());
    }

    #[test]
    fn does_not_declare_a_state_name_still_growing() {
        let mut parser = Parser::new();
        let parsed = parser.push("root = Text($d", false);
        assert_eq!(parsed.growing.as_deref(), Some("$d"));
        let parsed = parser.push("ish)", false);
        assert_eq!(parsed.growing, None);
        let parsed = Parser::new().push("root = Text($d + $d", false);
        assert_eq!(parsed.growing, None);
    }

    #[test]
    fn starts_again_when_the_text_changes_before_the_watermark() {
        let mut parser = Parser::new();
        let parsed = parser.push("Here you go:\na = 1\n", false);
        assert_eq!(names(&parsed), ["a"]);
        let parsed = parser.push("```openui\nroot = Text(\"x\")\n", false);
        assert_eq!(names(&parsed), ["root"]);
        let parsed = parser.set("b = 2\n", true);
        assert_eq!(names(&parsed), ["b"]);
        let parsed = parser.push("c = 3", true);
        assert_eq!(names(&parsed), ["c"], "a chunk after the end starts a new stream");
    }

    #[test]
    fn streams_to_what_a_batch_parse_reads() {
        let text = "Intro with \"quotes\n```openui\nroot = Stack([a, b, s]) // the page\n$n = -1\na = Text(\"line one\nline // two\", 'it\\'s')\n# note\nb = @Count([1 -2, -3])\ns = $n > 0\n  ? \"some\" :\n  \"none\"\nt = {k: [1, {x: \"}\"}], $s: 2}\n```\nOutro";
        let batch = parse(text);
        for size in 1..12 {
            let mut parser = Parser::new();
            let chars: Vec<char> = text.chars().collect();
            let chunks: Vec<String> = chars.chunks(size).map(|chunk| chunk.iter().collect()).collect();
            let mut parsed = Parsed::default();
            for (index, chunk) in chunks.iter().enumerate() {
                parsed = parser.push(chunk, index + 1 == chunks.len());
            }
            assert_eq!(names(&parsed), names(&batch), "chunks of {size}");
            for (name, statement) in &batch.statements {
                assert_eq!(&parsed.statements[name], statement, "chunks of {size}: {name}");
            }
            assert_eq!((parsed.incomplete, parsed.count), (batch.incomplete, batch.count));
        }
        assert_eq!(names(&batch), ["root", "$n", "a", "b", "s", "t"]);
    }
}
