//! The lexer (language.md §1.3). Lexing never fails: a character that matches no rule is skipped.

use crate::js;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Newline,
    LParen,
    RParen,
    LBrack,
    RBrack,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Equals,
    EqEq,
    NotEq,
    Greater,
    GreaterEq,
    Less,
    LessEq,
    And,
    Or,
    Not,
    Question,
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    True,
    False,
    Null,
    Str(String),
    Num(f64),
    /// A name with a lowercase first letter or `_`: a reference.
    Ident(String),
    /// A name with an uppercase first letter: a component, or a reference.
    Type(String),
    /// `$name`, with the `$`.
    State(String),
    /// `@name`, without the `@`.
    At(String),
}

impl Tok {
    /// A token after which `-` is subtraction: a literal, a name, a call's head, `)` or `]`.
    fn is_value(&self) -> bool {
        matches!(
            self,
            Tok::Num(_)
                | Tok::Str(_)
                | Tok::Ident(_)
                | Tok::Type(_)
                | Tok::RParen
                | Tok::RBrack
                | Tok::True
                | Tok::False
                | Tok::Null
                | Tok::State(_)
                | Tok::At(_)
        )
    }
}

/// A token and the bytes of the source it was read from.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub start: usize,
    pub end: usize,
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

pub fn tokenize(src: &str) -> Vec<Token> {
    let bytes = src.as_bytes();
    let mut tokens: Vec<Token> = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let byte = bytes[at];
        let next = bytes.get(at + 1).copied();
        let single = match byte {
            b' ' | b'\t' | b'\r' => {
                at += 1;
                continue;
            }
            b'\n' => Some(Tok::Newline),
            b'(' => Some(Tok::LParen),
            b')' => Some(Tok::RParen),
            b'[' => Some(Tok::LBrack),
            b']' => Some(Tok::RBrack),
            b'{' => Some(Tok::LBrace),
            b'}' => Some(Tok::RBrace),
            b',' => Some(Tok::Comma),
            b':' => Some(Tok::Colon),
            b'.' => Some(Tok::Dot),
            b'?' => Some(Tok::Question),
            b'+' => Some(Tok::Plus),
            b'*' => Some(Tok::Star),
            b'/' => Some(Tok::Slash),
            b'%' => Some(Tok::Percent),
            _ => None,
        };
        if let Some(tok) = single {
            at += 1;
            tokens.push(Token { tok, start, end: at });
            continue;
        }
        let pair = match (byte, next) {
            (b'=', Some(b'=')) => Some((Tok::EqEq, 2)),
            (b'=', _) => Some((Tok::Equals, 1)),
            (b'!', Some(b'=')) => Some((Tok::NotEq, 2)),
            (b'!', _) => Some((Tok::Not, 1)),
            (b'>', Some(b'=')) => Some((Tok::GreaterEq, 2)),
            (b'>', _) => Some((Tok::Greater, 1)),
            (b'<', Some(b'=')) => Some((Tok::LessEq, 2)),
            (b'<', _) => Some((Tok::Less, 1)),
            (b'&', Some(b'&')) => Some((Tok::And, 2)),
            (b'&', _) => Some((Tok::And, 1)),
            (b'|', Some(b'|')) => Some((Tok::Or, 2)),
            (b'|', _) => Some((Tok::Or, 1)),
            _ => None,
        };
        if let Some((tok, len)) = pair {
            at += len;
            tokens.push(Token { tok, start, end: at });
            continue;
        }
        let digit_next = next.is_some_and(|b| b.is_ascii_digit());
        let tok = match byte {
            b'"' => {
                let (text, end) = double_quoted(src, at);
                at = end;
                Tok::Str(text)
            }
            b'\'' => {
                let (text, end) = single_quoted(src, at);
                at = end;
                Tok::Str(text)
            }
            b'-' if digit_next && !tokens.last().is_some_and(|token| token.tok.is_value()) => {
                at = number_end(bytes, at + 1);
                Tok::Num(js::string_to_number(&src[start..at]))
            }
            b'-' => {
                at += 1;
                Tok::Minus
            }
            b'0'..=b'9' => {
                at = number_end(bytes, at);
                Tok::Num(js::string_to_number(&src[start..at]))
            }
            b'$' | b'@' if next.is_some_and(is_name_start) => {
                at += 1;
                while at < bytes.len() && is_name(bytes[at]) {
                    at += 1;
                }
                if byte == b'$' {
                    Tok::State(src[start..at].to_owned())
                } else {
                    Tok::At(src[start + 1..at].to_owned())
                }
            }
            byte if is_name_start(byte) => {
                while at < bytes.len() && is_name(bytes[at]) {
                    at += 1;
                }
                match &src[start..at] {
                    "true" => Tok::True,
                    "false" => Tok::False,
                    "null" => Tok::Null,
                    word if byte.is_ascii_uppercase() => Tok::Type(word.to_owned()),
                    word => Tok::Ident(word.to_owned()),
                }
            }
            _ => {
                at += src[at..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
        };
        tokens.push(Token { tok, start, end: at });
    }
    tokens
}

/// The end of `[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]*)?` from `at`.
fn number_end(bytes: &[u8], mut at: usize) -> usize {
    let digits = |mut at: usize| {
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
        }
        at
    };
    at = digits(at);
    if bytes.get(at) == Some(&b'.') && bytes.get(at + 1).is_some_and(u8::is_ascii_digit) {
        at = digits(at + 1);
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        at = digits(at);
    }
    at
}

/// A double-quoted string from `at`: JSON escapes, any other escaped character kept with its
/// backslash. One that never closes runs to the end.
fn double_quoted(src: &str, at: usize) -> (String, usize) {
    let mut out = String::new();
    let mut chars = src[at + 1..].char_indices().peekable();
    let base = at + 1;
    while let Some((offset, c)) = chars.next() {
        match c {
            '"' => return (out, base + offset + 1),
            '\\' => {
                let Some((_, escaped)) = chars.next() else {
                    out.push('\\');
                    break;
                };
                match escaped {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let rest = &src[base + offset + 2..];
                        match unicode_escape(rest) {
                            Some((c, len)) => {
                                out.push(c);
                                for _ in 0..len {
                                    chars.next();
                                }
                            }
                            None => out.push_str("\\u"),
                        }
                    }
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
            }
            c => out.push(c),
        }
    }
    (out, src.len())
}

/// `XXXX` after `\u`, with a `\uXXXX` low surrogate after a high one: the character and the
/// number of bytes read. A lone surrogate is U+FFFD.
fn unicode_escape(rest: &str) -> Option<(char, usize)> {
    let unit = |text: &str| -> Option<u16> {
        let hex = text.get(..4)?;
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u16::from_str_radix(hex, 16).ok()
    };
    let high = unit(rest)?;
    if !(0xd800..0xdc00).contains(&high) {
        return Some((char::from_u32(high.into()).unwrap_or('\u{fffd}'), 4));
    }
    let low = rest.get(4..).and_then(|after| after.strip_prefix("\\u")).and_then(unit);
    match low {
        Some(low) if (0xdc00..0xe000).contains(&low) => {
            let code = 0x10000 + ((u32::from(high) - 0xd800) << 10) + (u32::from(low) - 0xdc00);
            Some((char::from_u32(code).unwrap_or('\u{fffd}'), 10))
        }
        _ => Some(('\u{fffd}', 4)),
    }
}

/// A single-quoted string from `at`: `\'`, `\\`, `\n` and `\t`; any other escaped character is
/// kept without its backslash.
fn single_quoted(src: &str, at: usize) -> (String, usize) {
    let mut out = String::new();
    let mut chars = src[at + 1..].char_indices();
    let base = at + 1;
    while let Some((offset, c)) = chars.next() {
        match c {
            '\'' => return (out, base + offset + 1),
            '\\' => match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, escaped)) => out.push(escaped),
                None => break,
            },
            c => out.push(c),
        }
    }
    (out, src.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        tokenize(src).into_iter().map(|token| token.tok).collect()
    }

    #[test]
    fn reads_minus_by_what_comes_before() {
        assert_eq!(
            toks("[1 -2, -5]"),
            [Tok::LBrack, Tok::Num(1.0), Tok::Minus, Tok::Num(2.0), Tok::Comma, Tok::Num(-5.0), Tok::RBrack]
        );
        assert_eq!(
            toks("{a: 1} -2"),
            [Tok::LBrace, Tok::Ident("a".into()), Tok::Colon, Tok::Num(1.0), Tok::RBrace, Tok::Num(-2.0)]
        );
        assert_eq!(toks("x-1"), [Tok::Ident("x".into()), Tok::Minus, Tok::Num(1.0)]);
    }

    #[test]
    fn reads_numbers_names_and_skips_the_rest() {
        assert_eq!(
            toks("1. m.0.1 2e3 a;b é $ @ $x @Count"),
            [
                Tok::Num(1.0),
                Tok::Dot,
                Tok::Ident("m".into()),
                Tok::Dot,
                Tok::Num(0.1),
                Tok::Num(2000.0),
                Tok::Ident("a".into()),
                Tok::Ident("b".into()),
                Tok::State("$x".into()),
                Tok::At("Count".into())
            ]
        );
        assert!(matches!(toks("1e")[..], [Tok::Num(value)] if value.is_nan()));
        assert_eq!(
            toks("a & b | c\r\n"),
            [Tok::Ident("a".into()), Tok::And, Tok::Ident("b".into()), Tok::Or, Tok::Ident("c".into()), Tok::Newline]
        );
    }

    #[test]
    fn decodes_escapes() {
        assert_eq!(toks(r#""q\"d\\b\ttA""#), [Tok::Str("q\"d\\b\ttA".into())]);
        assert_eq!(
            toks(r"'it\'s' 'x\ny' 'a\qb'"),
            [Tok::Str("it's".into()), Tok::Str("x\ny".into()), Tok::Str("aqb".into())]
        );
        assert_eq!(toks("\"one\ntwo\\n\""), [Tok::Str("one\ntwo\n".into())]);
        assert_eq!(toks(r#""😀 \ud83d \q""#), [Tok::Str("😀 \u{fffd} \\q".into())]);
        assert_eq!(toks("\"open"), [Tok::Str("open".into())]);
    }
}
