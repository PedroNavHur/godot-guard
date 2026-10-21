//! The Python built-ins the formatter's output depends on, with Python's exact semantics.

use std::ops::Range;

/// `str.isspace` for one character, which is also what `\s` matches in a `str` regex.
/// Unicode White_Space plus the four ASCII separators U+001C..U+001F.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `len(s)`: code points, not bytes.
pub fn len(s: &str) -> i64 {
    s.chars().count() as i64
}

/// `str.splitlines()`: splits on every line boundary Python knows, `\r\n` counting as one, and drops a
/// trailing empty line.
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\u{0b}'
                | '\u{0c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        );
        if !is_break {
            continue;
        }
        lines.push(&s[start..at]);
        start = at + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|&(_, next)| next == '\n') {
            chars.next();
            start += 1;
        }
    }
    if start < s.len() {
        lines.push(&s[start..]);
    }
    lines
}

/// The index range of `seq[start:end]` for a sequence of `len` items: negative indices count from the
/// end, and both bounds clamp, as Python slicing does.
pub fn slice(len: usize, start: Option<i64>, end: Option<i64>) -> Range<usize> {
    let bound = |index: Option<i64>, default: usize| match index {
        None => default,
        Some(i) if i < 0 => (len as i64 + i).max(0) as usize,
        Some(i) => (i as usize).min(len),
    };
    let start = bound(start, 0);
    let end = bound(end, len);
    start..end.max(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_matches_python() {
        assert_eq!(
            splitlines("a\nb\r\nc\rd\u{0c}e\n"),
            ["a", "b", "c", "d", "e"]
        );
        assert_eq!(splitlines("a\n\n"), ["a", ""]);
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert_eq!(splitlines("\u{2028}x"), ["", "x"]);
    }

    #[test]
    fn slice_matches_python() {
        assert_eq!(slice(5, Some(1), Some(-1)), 1..4);
        assert_eq!(slice(5, Some(-1), None), 4..5);
        assert_eq!(slice(5, Some(7), Some(9)), 5..5);
        assert_eq!(slice(5, Some(3), Some(1)), 3..3);
        assert_eq!(slice(5, None, Some(-9)), 0..0);
    }

    #[test]
    fn space_matches_python() {
        assert!(is_space('\u{1f}') && is_space('\u{a0}') && !is_space('\u{200b}'));
        assert_eq!(strip("\u{1c} a \t"), "a");
    }
}
