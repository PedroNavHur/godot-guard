//! Python `str` behaviour the line-based code depends on: `splitlines`, `isspace` and `strip`.

/// `str.isspace` for one character, which is also what `\s` matches in a `str` pattern: Unicode
/// White_Space plus the four information separators U+001C..U+001F.
pub fn is_python_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `str.strip()`.
pub fn python_strip(text: &str) -> &str {
    text.trim_matches(is_python_space)
}

/// `str.splitlines()`: lines split at `\n`, `\r`, `\r\n`, `\v`, `\f`, U+001C..U+001E, U+0085, U+2028 and
/// U+2029, without their ends, and no empty last line for a trailing end.
pub fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        let is_break = matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
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
        lines.push(&text[start..index]);
        start = index + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|&(_, next)| next == '\n') {
            chars.next();
            start += 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// The text Python's `open(path, encoding="utf-8").read()` returns: universal newlines turn `\r\n` and a
/// lone `\r` into `\n`.
pub fn universal_newlines(text: String) -> String {
    if !text.contains('\r') {
        return text;
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_matches_python() {
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert_eq!(splitlines("a\n"), ["a"]);
        assert_eq!(splitlines("a\n\nb"), ["a", "", "b"]);
        assert_eq!(splitlines("a\r\nb\rc\u{2028}d\u{c}"), ["a", "b", "c", "d"]);
        assert_eq!(splitlines("a\u{1f}b"), ["a\u{1f}b"]);
    }

    #[test]
    fn strip_and_newlines_match_python() {
        assert_eq!(python_strip("\u{1f} x \u{3000}"), "x");
        assert_eq!(universal_newlines("a\r\nb\rc\n".into()), "a\nb\nc\n");
    }
}
