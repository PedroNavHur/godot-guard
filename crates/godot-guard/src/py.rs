//! Python's string semantics where godot-guard 0.4.0's output depends on them.
//!
//! The Python reads text with universal newlines, splits lines with `str.splitlines`, strips with
//! `str.strip`, and quotes with `repr`; these reproduce each one.

use std::fs;
use std::io;
use std::path::Path;

/// `Path.read_text(encoding="utf-8", errors="replace")`: invalid UTF-8 becomes U+FFFD, and `\r\n` and
/// a lone `\r` become `\n`.
pub fn read_text(path: &Path) -> io::Result<String> {
    Ok(decode_text(&fs::read(path)?))
}

/// Text-mode decoding of `bytes`, as `read_text`.
pub fn decode_text(bytes: &[u8]) -> String {
    translate_newlines(&String::from_utf8_lossy(bytes))
}

/// Universal newlines: `\r\n` and a lone `\r` become `\n`.
pub fn translate_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_owned();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Whether `str.splitlines` breaks a line at `c`.
fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r'
            | '\x0b'
            | '\x0c'
            | '\x1c'
            | '\x1d'
            | '\x1e'
            | '\u{85}'
            | '\u{2028}'
            | '\u{2029}'
    )
}

/// `str.splitlines()`: lines without their breaks; `\r\n` is one break, and a final break adds no line.
pub fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        if !is_line_break(c) {
            continue;
        }
        lines.push(&text[start..index]);
        start = index + c.len_utf8();
        if c == '\r' && chars.peek().map(|&(_, next)| next) == Some('\n') {
            chars.next();
            start += 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// `str.isspace()` for one character: Rust's whitespace plus the four ASCII separators Python counts.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// `str.strip()`.
pub fn strip(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// `repr(text)` for a `str`.
pub fn repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push(quote);
    for c in text.chars() {
        push_escaped(&mut quoted, c, quote);
    }
    quoted.push(quote);
    quoted
}

/// `unicode_repr`'s treatment of one character inside `quote`s.
fn push_escaped(quoted: &mut String, c: char, quote: char) {
    match c {
        '\\' => quoted.push_str("\\\\"),
        '\t' => quoted.push_str("\\t"),
        '\n' => quoted.push_str("\\n"),
        '\r' => quoted.push_str("\\r"),
        _ if c == quote => {
            quoted.push('\\');
            quoted.push(c);
        }
        _ if is_printable(c) => quoted.push(c),
        _ if (c as u32) < 0x100 => quoted.push_str(&format!("\\x{:02x}", c as u32)),
        _ if (c as u32) < 0x10000 => quoted.push_str(&format!("\\u{:04x}", c as u32)),
        _ => quoted.push_str(&format!("\\U{:08x}", c as u32)),
    }
}

/// `str.isprintable()` for one character.
///
/// ceiling: Python excludes every code point in categories Cc, Cf, Cs, Co, Cn, Zl, Zp, and Zs but the
/// space; without Unicode tables this covers controls, the separators, private use, and the common format
/// characters, so an unassigned or rare format code point prints raw where Python escapes it.
fn is_printable(c: char) -> bool {
    let code = c as u32;
    let separator = c != ' ' && c.is_whitespace();
    let format = matches!(code, 0xad | 0x600..=0x605 | 0x61c | 0x6dd | 0x70f | 0x180e | 0x200b..=0x200f
        | 0x202a..=0x202e | 0x2060..=0x2064 | 0x2066..=0x206f | 0xfeff | 0xfff9..=0xfffb);
    let private = matches!(code, 0xe000..=0xf8ff | 0xf0000..=0xffffd | 0x100000..=0x10fffd);
    !(c.is_control() || separator || format || private)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_matches_python() {
        assert_eq!(
            splitlines("a\nb\r\nc\rd\x0be\u{2028}f\n"),
            ["a", "b", "c", "d", "e", "f"]
        );
        assert_eq!(splitlines("a\n\nb"), ["a", "", "b"]);
        assert!(splitlines("").is_empty());
        assert_eq!(splitlines("\n"), [""]);
    }

    #[test]
    fn newlines_are_translated() {
        assert_eq!(decode_text(b"a\r\nb\rc\n\xff"), "a\nb\nc\n\u{fffd}");
    }

    #[test]
    fn repr_matches_python() {
        assert_eq!(repr("a.gd::gone"), "'a.gd::gone'");
        assert_eq!(repr("it's"), "\"it's\"");
        assert_eq!(repr("'\""), "'\\'\"'");
        assert_eq!(
            repr("tab\there\\\x01\u{a0}é"),
            "'tab\\there\\\\\\x01\\xa0é'"
        );
    }

    #[test]
    fn strip_counts_python_whitespace() {
        assert_eq!(strip("\x1c a \u{3000}"), "a");
    }
}
