//! Hand-written matchers for the grammar's regex terminals, each returning what Python's `regex` module
//! returns for `pattern.match(text, pos)`: the byte length of the first match a backtracking engine finds.
//!
//! `tools/gdscript/gen_tables.py` maps each regex source lark compiles to one `Matcher`, and refuses an
//! unknown source. Every pattern here is deterministic once its optional parts are read greedily, which
//! the comments on the trickier ones argue; the corpus parity test is the proof.

use crate::tables::Matcher;

/// The byte length of `matcher`'s match at the start of `text`, if any. Zero-width matches cannot happen:
/// lark rejects zero-width terminals.
pub fn match_at(matcher: Matcher, text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let len = match matcher {
        Matcher::WsInline => run(bytes, 0, |b| b == b' ' || b == b'\t'),
        Matcher::Hex => radix(bytes, b'x', |b| b.is_ascii_hexdigit()),
        Matcher::Bin => radix(bytes, b'b', |b| b == b'0' || b == b'1'),
        Matcher::TypeHint => type_hint(text),
        Matcher::Number => number(bytes),
        Matcher::LongString => long_string(bytes, 0),
        Matcher::LongRString => raw_prefix(bytes, long_string),
        Matcher::RegularString => regular_string(bytes, 0),
        Matcher::RegularRString => raw_prefix(bytes, regular_string),
        Matcher::Newline => newline(bytes),
        Matcher::Comment => comment(bytes),
        Matcher::LineCont => line_cont(bytes),
        Matcher::Name => name(text, 0),
        Matcher::Anything => run(bytes, 0, |b| !matches!(b, b'#' | b'"' | b'\'')),
    };
    Some(len).filter(|&len| len > 0)
}

/// Whether a match can start with a byte of `class`: an ASCII byte, or `NON_ASCII` for any lead byte of a
/// multi-byte character. Lets each lexer try only the terminals that could match.
pub fn can_start(matcher: Matcher, class: usize) -> bool {
    if class == NON_ASCII {
        return matches!(
            matcher,
            Matcher::Name | Matcher::TypeHint | Matcher::Anything
        );
    }
    let b = class as u8;
    match matcher {
        Matcher::WsInline => b == b' ' || b == b'\t',
        Matcher::Hex | Matcher::Bin => b == b'-' || b == b'0',
        Matcher::TypeHint | Matcher::Name => name_start(b as char),
        Matcher::Number => matches!(b, b'+' | b'-' | b'.' | b'0'..=b'9'),
        Matcher::LongString | Matcher::RegularString => b == b'"' || b == b'\'',
        Matcher::LongRString | Matcher::RegularRString => b == b'r' || b == b'R',
        Matcher::Newline => matches!(b, b'\r' | b'\n' | b'#'),
        Matcher::Comment => b == b'#',
        Matcher::LineCont => b == b'\\',
        Matcher::Anything => !matches!(b, b'#' | b'"' | b'\''),
    }
}

pub const NON_ASCII: usize = 128;

/// The longest run of bytes from `start` that satisfy `accept`, as a length from 0.
fn run(bytes: &[u8], start: usize, accept: impl Fn(u8) -> bool) -> usize {
    start
        + bytes[start.min(bytes.len())..]
            .iter()
            .take_while(|&&b| accept(b))
            .count()
}

/// `-?0x[a-fA-F0-9][a-fA-F0-9_]*` and `-?0b[01][01_]*`. Without the sign the match would need `0` where
/// the `-` is, so the optional sign never backtracks.
fn radix(bytes: &[u8], marker: u8, digit: impl Fn(u8) -> bool) -> usize {
    let start = usize::from(bytes.first() == Some(&b'-'));
    if bytes.get(start) != Some(&b'0') || bytes.get(start + 1) != Some(&marker) {
        return 0;
    }
    match bytes.get(start + 2) {
        Some(&b) if digit(b) => run(bytes, start + 3, |b| digit(b) || b == b'_'),
        _ => 0,
    }
}

/// `INT: [0-9](?:[0-9]|_)*`, from `at`; 0 when there is no digit.
fn int(bytes: &[u8], at: usize) -> usize {
    match bytes.get(at) {
        Some(b) if b.is_ascii_digit() => {
            run(bytes, at + 1, |b| b.is_ascii_digit() || b == b'_') - at
        }
        _ => 0,
    }
}

/// `(?:e|E)(?:\+|\-)?INT`, from `at`; 0 when absent. A sign without digits after it fails, and without
/// the sign the digits would have to start at the sign, so the optional sign never backtracks.
fn exponent(bytes: &[u8], at: usize) -> usize {
    if !matches!(bytes.get(at), Some(b'e' | b'E')) {
        return 0;
    }
    let sign = usize::from(matches!(bytes.get(at + 1), Some(b'+' | b'-')));
    match int(bytes, at + 1 + sign) {
        0 => 0,
        digits => 1 + sign + digits,
    }
}

/// `[+-]? (INT EXP | (INT "." INT? | "." INT) EXP? | INT)`, alternatives tried in that order. Each INT is
/// greedy: a shorter one would leave a digit or `_` where the next part needs `e`, `.`, or the end.
fn number(bytes: &[u8]) -> usize {
    let start = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let digits = int(bytes, start);
    if digits > 0 {
        let exp = exponent(bytes, start + digits);
        if exp > 0 {
            return start + digits + exp;
        }
        if bytes.get(start + digits) == Some(&b'.') {
            let at = start + digits + 1;
            let fraction = int(bytes, at);
            return at + fraction + exponent(bytes, at + fraction);
        }
        return start + digits;
    }
    if bytes.get(start) == Some(&b'.') {
        let fraction = int(bytes, start + 1);
        if fraction > 0 {
            let at = start + 1 + fraction;
            return at + exponent(bytes, at);
        }
    }
    0
}

/// An `r` string: `(?i)` lets the prefix be `R` too.
fn raw_prefix(bytes: &[u8], string: fn(&[u8], usize) -> usize) -> usize {
    match bytes.first() {
        Some(b'r' | b'R') => string(bytes, 1),
        _ => 0,
    }
}

/// The quoted part of `(?i:("(?!"").*?(?<!\\)(\\\\)*?"|'(?!'').*?(?<!\\)(\\\\)*?'))`, from `open`.
/// `.` stops at `\n`. The lazy body grows one character at a time; at each candidate end that does not
/// follow a backslash, the lazy `(\\\\)*?` tries 0, 1, 2… escaped-backslash pairs before the quote.
fn regular_string(bytes: &[u8], open: usize) -> usize {
    let quote = match bytes.get(open) {
        Some(&q @ (b'"' | b'\'')) => q,
        _ => return 0,
    };
    if bytes.get(open + 1) == Some(&quote) && bytes.get(open + 2) == Some(&quote) {
        return 0;
    }
    closing(bytes, open + 1, &[quote], false)
}

/// `"""…"""` or `'''…'''` with the same escape rule as `regular_string`, `.` matching newlines.
fn long_string(bytes: &[u8], open: usize) -> usize {
    let quote = match bytes.get(open) {
        Some(&q @ (b'"' | b'\'')) => q,
        _ => return 0,
    };
    let delimiter = [quote; 3];
    if bytes.get(open..open + 3) != Some(&delimiter[..]) {
        return 0;
    }
    closing(bytes, open + 3, &delimiter, true)
}

/// The end of `.*?(?<!\\)(\\\\)*?DELIMITER` starting at `body`, or 0 when it does not match.
fn closing(bytes: &[u8], body: usize, delimiter: &[u8], dotall: bool) -> usize {
    let mut end = body;
    loop {
        if bytes[end - 1] != b'\\' {
            let mut at = end;
            loop {
                if bytes[at..].starts_with(delimiter) {
                    return at + delimiter.len();
                }
                if !bytes[at..].starts_with(b"\\\\") {
                    break;
                }
                at += 2;
            }
        }
        match bytes.get(end) {
            None => return 0,
            Some(b'\n') if !dotall => return 0,
            Some(_) => end += 1,
        }
    }
}

/// `(?:\r?\n[\t ]*|#[^\n]*)+`, each repetition greedy; nothing follows, so nothing backtracks.
fn newline(bytes: &[u8]) -> usize {
    let mut at = 0;
    loop {
        let rest = &bytes[at..];
        if rest.starts_with(b"\n") || rest.starts_with(b"\r\n") {
            let newline = if rest[0] == b'\r' { 2 } else { 1 };
            at = run(bytes, at + newline, |b| b == b'\t' || b == b' ');
        } else if rest.first() == Some(&b'#') {
            at = run(bytes, at + 1, |b| b != b'\n');
        } else {
            return at;
        }
    }
}

fn comment(bytes: &[u8]) -> usize {
    match bytes.first() {
        Some(b'#') => run(bytes, 1, |b| b != b'\n'),
        _ => 0,
    }
}

/// `\\[\t \f]*\r?\n`; the class holds neither `\r` nor `\n`, so it never gives characters back.
fn line_cont(bytes: &[u8]) -> usize {
    if bytes.first() != Some(&b'\\') {
        return 0;
    }
    let at = run(bytes, 1, |b| matches!(b, b'\t' | b' ' | b'\x0c'));
    match &bytes[at..] {
        [b'\n', ..] => at + 1,
        [b'\r', b'\n', ..] => at + 2,
        _ => 0,
    }
}

/// NAME, `[\p{XID_Start}_ͺ-ͽ゛-ゟﹰ-ﹴﹶ-ﻼ\x00]` then
/// `[\p{XID_Continue}_ͺ-ͽﹶ-ﻼ\x00]*`, from byte `at`; 0 when absent.
fn name(text: &str, at: usize) -> usize {
    let mut chars = text[at..].char_indices();
    match chars.next() {
        Some((_, c)) if name_start(c) => {}
        _ => return 0,
    }
    for (offset, c) in chars {
        if !name_continue(c) {
            return offset;
        }
    }
    text.len() - at
}

fn name_start(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphabetic() || c == '_' || c == '\0';
    }
    unicode_ident::is_xid_start(c)
        || matches!(c, '\u{37a}'..='\u{37d}' | '\u{309b}'..='\u{309f}' | '\u{fe70}'..='\u{fe74}' | '\u{fe76}'..='\u{fefc}')
}

fn name_continue(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || c == '_' || c == '\0';
    }
    unicode_ident::is_xid_continue(c)
        || matches!(c, '\u{37a}'..='\u{37d}' | '\u{fe76}'..='\u{fefc}')
}

/// TYPE, `NAME ((?:\ )?\.(?:\ )?NAME)*`, from `at`; 0 when absent. Each repetition can match one way
/// only: a space is taken when the dot (or the name) follows it, and a shorter NAME would leave a
/// character that neither `.`, ` `, `[`, `,` nor `]` can follow.
fn type_name(text: &str, at: usize) -> usize {
    let bytes = text.as_bytes();
    let mut end = at + name(text, at);
    if end == at {
        return 0;
    }
    loop {
        let dot = end + usize::from(bytes[end..].starts_with(b" ."));
        if bytes.get(dot) != Some(&b'.') {
            return end - at;
        }
        match spaced_name(text, dot + 1, name) {
            Some(next) => end = next,
            None => return end - at,
        }
    }
}

/// `(?:\ )?PART` from `at`: the end of PART after an optional space, which is taken only when PART
/// matches after it (PART cannot start with a space).
fn spaced_name(text: &str, at: usize, part: fn(&str, usize) -> usize) -> Option<usize> {
    if text.as_bytes().get(at) == Some(&b' ') {
        let len = part(text, at + 1);
        if len > 0 {
            return Some(at + 1 + len);
        }
    }
    Some(at + part(text, at)).filter(|&end| end > at)
}

/// TYPE_HINT, `TYPE (?:\[TYPE(?:(?:\ )?,(?:\ )?TYPE)?\])?`. The bracket group is all or nothing: after a
/// greedy TYPE only `]`, `,` or ` ,` can continue, and fewer TYPE repetitions leave `.` or ` .` there.
fn type_hint(text: &str) -> usize {
    let bytes = text.as_bytes();
    let base = type_name(text, 0);
    if base == 0 || bytes.get(base) != Some(&b'[') {
        return base;
    }
    let first = base + 1 + type_name(text, base + 1);
    if first == base + 1 {
        return base;
    }
    if bytes.get(first) == Some(&b']') {
        return first + 1;
    }
    let comma = first + usize::from(bytes[first..].starts_with(b" ,"));
    if bytes.get(comma) != Some(&b',') {
        return base;
    }
    match spaced_name(text, comma + 1, type_name) {
        Some(second) if bytes.get(second) == Some(&b']') => second + 1,
        _ => base,
    }
}

#[cfg(test)]
mod tests;
