//! `fnmatch.fnmatchcase` as Python 3.9 translates it: `*` crosses `/`, `?` is one character, `[...]` is
//! a set (`[!...]` negated), and a `[` without its `]` is literal. There is no way to quote.

use regex::Regex;

/// A compiled pattern, kept with its text for the places the text is passed on.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub text: String,
    regex: Regex,
}

impl Pattern {
    /// The pattern, or the reason Python's `re` would reject its translation (a reversed range).
    pub fn new(text: &str) -> Result<Pattern, String> {
        let translated = translate(text)?;
        let regex = Regex::new(&format!(r"(?s)\A(?:{translated})\z")).map_err(|e| e.to_string())?;
        Ok(Pattern {
            text: text.to_owned(),
            regex,
        })
    }

    pub fn matches(&self, name: &str) -> bool {
        self.regex.is_match(name)
    }
}

/// Port of `fnmatch.translate`, emitting Rust regex syntax.
fn translate(pattern: &str) -> Result<String, String> {
    let chars: Vec<char> = pattern.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        i += 1;
        match c {
            '*' if !out.ends_with(".*") => out.push_str(".*"),
            '*' => {}
            '?' => out.push('.'),
            '[' => match set_end(&chars, i) {
                None => out.push_str(r"\["),
                Some(j) => {
                    out.push_str(&set_regex(&set_stuff(&chars, i, j))?);
                    i = j + 1;
                }
            },
            _ => out.push_str(&regex::escape(c.encode_utf8(&mut [0; 4]))),
        }
    }
    Ok(out)
}

/// The index of the `]` closing a set opened just before `i`, or None when it is never closed.
fn set_end(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if chars.get(j) == Some(&'!') {
        j += 1;
    }
    if chars.get(j) == Some(&']') {
        j += 1;
    }
    while j < chars.len() && chars[j] != ']' {
        j += 1;
    }
    (j < chars.len()).then_some(j)
}

/// The set's body as fnmatch rewrites it for `re`: backslashes doubled, and, when it holds `--`, hyphens
/// escaped except those that form ranges; then `&`, `~`, and `|` escaped, and `!` turned into `^`.
fn set_stuff(chars: &[char], mut i: usize, j: usize) -> String {
    let raw: String = chars[i..j].iter().collect();
    let mut stuff = if !raw.contains("--") {
        raw.replace('\\', r"\\")
    } else {
        let mut chunks = Vec::new();
        let mut k = if chars[i] == '!' { i + 2 } else { i + 1 };
        while let Some(found) = (k..j).find(|&index| chars[index] == '-') {
            chunks.push(chars[i..found].iter().collect::<String>());
            i = found + 1;
            k = found + 3;
        }
        chunks.push(chars[i..j].iter().collect());
        let escaped: Vec<String> = chunks
            .iter()
            .map(|s| s.replace('\\', r"\\").replace('-', r"\-"))
            .collect();
        escaped.join("-")
    };
    stuff = stuff
        .replace('&', r"\&")
        .replace('~', r"\~")
        .replace('|', r"\|");
    if let Some(rest) = stuff.strip_prefix('!') {
        format!("^{rest}")
    } else if stuff.starts_with(['^', '[']) {
        format!("\\{stuff}")
    } else {
        stuff
    }
}

/// The Rust class for a set body in `re` syntax, read as `sre_parse` reads it.
fn set_regex(stuff: &str) -> Result<String, String> {
    let chars: Vec<char> = stuff.chars().collect();
    let negated = chars.first() == Some(&'^');
    let mut i = usize::from(negated);
    let mut class = String::from(if negated { "[^" } else { "[" });
    while i < chars.len() {
        let (low, next) = set_item(&chars, i);
        i = next;
        let mut high = low;
        if chars.get(i) == Some(&'-') && i + 1 < chars.len() {
            (high, i) = set_item(&chars, i + 1);
            if high < low {
                return Err(format!("bad character range {low}-{high}"));
            }
        }
        class.push_str(&format!("\\x{{{:x}}}-\\x{{{:x}}}", low as u32, high as u32));
    }
    class.push(']');
    Ok(class)
}

/// One character of a set, after its escaping backslash if any, and the index past it.
fn set_item(chars: &[char], i: usize) -> (char, usize) {
    match chars[i] {
        '\\' if i + 1 < chars.len() => (chars[i + 1], i + 2),
        c => (c, i + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::Pattern;

    fn matches(pattern: &str, name: &str) -> bool {
        Pattern::new(pattern).unwrap().matches(name)
    }

    #[test]
    fn star_crosses_slashes_and_question_is_one_character() {
        assert!(matches("addons/**", "addons/x/y.gd"));
        assert!(matches("*.gd", "a/b.gd"));
        assert!(!matches("addons/**", "addons"));
        assert!(matches("a?c", "abc"));
        assert!(!matches("a?c", "ac"));
    }

    #[test]
    fn sets_follow_python() {
        assert!(matches("[ab].gd", "b.gd"));
        assert!(matches("[!ab].gd", "c.gd"));
        assert!(!matches("[!ab].gd", "a.gd"));
        assert!(matches("[]]", "]"));
        assert!(matches("[!]]", "a"));
        assert!(matches("[a-c]", "b"));
        assert!(matches("[a-]", "-"));
        assert!(matches("[--a]", "-"));
        assert!(matches("[!a-c-e]", "d"));
        assert!(matches("[a-c-e]", "-"));
        assert!(matches("[[]", "["));
        assert!(matches("[&~|]", "~"));
        assert!(matches("[\\]", "\\"));
        assert!(matches("[^]", "^"));
    }

    #[test]
    fn unclosed_set_is_literal() {
        assert!(matches("a[b", "a[b"));
        assert!(!matches("a[b", "ab"));
    }

    #[test]
    fn reversed_range_is_rejected() {
        assert!(Pattern::new("[z-a]").is_err());
        assert!(Pattern::new("[a--c]").is_err());
    }
}
