//! Name-check patterns: Python `re` syntax from a config, compiled for the `regex` crate with the same meaning.
//!
//! The translation reads the pattern as Python's `sre_parse` does and writes each construct in `regex` syntax,
//! so a character that is plain in Python but special in `regex` (`&&`, `--` and `~~` in a class, `[:alpha:]`,
//! `\<`) stays a literal, and `\w` and `\s` take Python's classes. A construct `regex` cannot express the
//! same way (backreferences, lookaround, word boundaries, case-insensitive, verbose or ASCII mode) is an
//! error, never an approximation. Names are NAME tokens, which hold no newline, so Python's `$`
//! (the end, or before a final newline) and `regex`'s `$` agree on them.

use std::fmt;

use regex::Regex;

#[derive(Debug, PartialEq)]
pub struct PatternError {
    pub pattern: String,
    pub reason: String,
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot use name pattern {:?}: {}",
            self.pattern, self.reason
        )
    }
}

impl std::error::Error for PatternError {}

/// A compiled pattern that answers `re.fullmatch(pattern, name) is not None`.
#[derive(Clone, Debug)]
pub struct FullMatch(Regex);

impl FullMatch {
    pub fn new(pattern: &str) -> Result<FullMatch, PatternError> {
        let fail = |reason: String| PatternError {
            pattern: pattern.to_owned(),
            reason,
        };
        let translated = translate(pattern).map_err(fail)?;
        let anchored = format!(r"\A(?:{translated})\z");
        Regex::new(&anchored)
            .map(FullMatch)
            .map_err(|e| fail(e.to_string()))
    }

    pub fn is_match(&self, name: &str) -> bool {
        self.0.is_match(name)
    }
}

/// What the translation last wrote, for Python's rules on repeats.
#[derive(Clone, Copy, PartialEq)]
enum Last {
    Other,
    /// A repeat that a `?` may still make lazy.
    Repeat,
    /// A lazy repeat.
    LazyRepeat,
}

/// `pattern` in `regex` syntax.
pub fn translate(pattern: &str) -> Result<String, String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut last = Last::Other;
    let mut i = 0;
    while i < chars.len() {
        let (text, next, is_repeat) = translate_item(&chars, i)?;
        last = next_last(last, chars[i], is_repeat)?;
        out.push_str(&text);
        i = next;
    }
    Ok(out)
}

/// The construct at `chars[i]` in `regex` syntax, the index after it, and whether it is a repeat.
fn translate_item(chars: &[char], i: usize) -> Result<(String, usize, bool), String> {
    let c = chars[i];
    let (text, next, is_repeat) = match c {
        '\\' => {
            let (text, next) = escape(chars, i, Context::Pattern)?;
            (text, next, false)
        }
        '[' => {
            let (text, next) = class(chars, i)?;
            (text, next, false)
        }
        '(' => {
            let (text, next) = group_open(chars, i)?;
            (text, next, false)
        }
        '*' | '+' | '?' => (c.to_string(), i + 1, true),
        '{' => match repeat(chars, i)? {
            Some((text, next)) => (text, next, true),
            None => (r"\{".to_owned(), i + 1, false),
        },
        ')' | '|' | '.' | '^' | '$' => (c.to_string(), i + 1, false),
        _ => {
            let mut text = String::new();
            push_literal(&mut text, c);
            (text, i + 1, false)
        }
    };
    Ok((text, next, is_repeat))
}

/// What the translation has last written once `c` is written.
fn next_last(last: Last, c: char, is_repeat: bool) -> Result<Last, String> {
    match (is_repeat, last) {
        (false, _) => Ok(Last::Other),
        (true, Last::Other) => Ok(Last::Repeat),
        (true, Last::Repeat) if c == '?' => Ok(Last::LazyRepeat),
        // Python 3.9 has no possessive repeats: `a*+`, `a**` and `a*??` are errors.
        (true, _) => Err("multiple repeat".into()),
    }
}

fn push_literal(out: &mut String, c: char) {
    if c.is_ascii_punctuation() || c == ' ' {
        out.push_str(&regex::escape(&c.to_string()));
    } else {
        out.push(c);
    }
}

/// Where an escape stands: in the pattern, or in a character class.
#[derive(Clone, Copy, PartialEq)]
enum Context {
    Pattern,
    Class,
}

/// The escape at `chars[i]` (a backslash) and the index after it. Inside a class, `\b` is a backspace.
fn escape(chars: &[char], i: usize, context: Context) -> Result<(String, usize), String> {
    let Some(&c) = chars.get(i + 1) else {
        return Err("bad escape (end of pattern)".into());
    };
    let simple = |text: &str| Ok((text.to_owned(), i + 2));
    match c {
        'd' | 'D' | 'w' | 'W' | 's' | 'S' => simple(&format!("[{}]", class_escape(c))),
        'b' | 'B' | 'A' | 'Z' => simple(context_escape(c, context)?),
        'a' | 'f' | 'n' | 'r' | 't' | 'v' => simple(control_escape(c)),
        'x' => hex_escape(chars, i, 2),
        'u' => hex_escape(chars, i, 4),
        'U' => hex_escape(chars, i, 8),
        '0'..='9' => Err(format!(
            "\\{c}: backreferences and octal escapes are not supported"
        )),
        c if c.is_ascii_alphanumeric() => Err(format!("bad escape \\{c}")),
        c => {
            let mut text = String::new();
            push_literal(&mut text, c);
            Ok((text, i + 2))
        }
    }
}

/// `\b`, `\B`, `\A` and `\Z`, which mean one thing in a pattern and another (or nothing) in a class.
fn context_escape(c: char, context: Context) -> Result<&'static str, String> {
    match (c, context) {
        ('b', Context::Class) => Ok(r"\x08"),
        ('b' | 'B', Context::Pattern) => Err(format!(
            "\\{c}: Python's word boundaries use its own \\w; not translated"
        )),
        ('A', Context::Pattern) => Ok(r"\A"),
        ('Z', Context::Pattern) => Ok(r"\z"),
        _ => Err(format!("bad escape \\{c}")),
    }
}

fn control_escape(c: char) -> &'static str {
    match c {
        'a' => r"\x07",
        'f' => r"\x0C",
        'n' => r"\n",
        'r' => r"\r",
        't' => r"\t",
        'v' => r"\x0B",
        _ => unreachable!("not a control escape: \\{c}"),
    }
}

/// A class escape as the members of a `regex` class. Python's `\\w` is `str.isalnum()` or `_`, so it takes
/// every letter and number (`²` too) but no marks or other connectors; its `\\s` is `str.isspace()`, which
/// adds U+001C..U+001F to Unicode White_Space; its `\\d` is Unicode Nd, as `regex`'s is.
fn class_escape(c: char) -> &'static str {
    match c {
        'w' => r"\p{L}\p{N}_",
        'W' => r"[^\p{L}\p{N}_]",
        's' => r"\s\x1C-\x1F",
        'S' => r"[^\s\x1C-\x1F]",
        'd' => r"\d",
        'D' => r"\D",
        _ => unreachable!("not a class escape: \\{c}"),
    }
}

fn hex_escape(chars: &[char], i: usize, width: usize) -> Result<(String, usize), String> {
    let digits: String = chars.iter().skip(i + 2).take(width).collect();
    if digits.chars().count() != width || !digits.chars().all(|d| d.is_ascii_hexdigit()) {
        return Err(format!("incomplete escape \\{}{digits}", chars[i + 1]));
    }
    let code = u32::from_str_radix(&digits, 16).map_err(|e| e.to_string())?;
    if char::from_u32(code).is_none() {
        return Err(format!("bad escape \\{}{digits}", chars[i + 1]));
    }
    Ok((format!("\\x{{{digits}}}"), i + 2 + width))
}

/// `{m}`, `{m,}`, `{,n}` or `{m,n}` at `chars[i]`, as `regex` writes it; None when Python reads `{` as a literal.
fn repeat(chars: &[char], i: usize) -> Result<Option<(String, usize)>, String> {
    let Some(close) = chars[i..].iter().position(|&c| c == '}').map(|p| p + i) else {
        return Ok(None);
    };
    let body: String = chars[i + 1..close].iter().collect();
    let Some((low, high)) = repeat_bounds(&body) else {
        return Ok(None);
    };
    let low = if low.is_empty() { "0" } else { low };
    check_bounds(low, high)?;
    let text = match high {
        None => format!("{{{low}}}"),
        Some(high) => format!("{{{low},{high}}}"),
    };
    Ok(Some((text, close + 1)))
}

/// The bounds inside `{...}`; None when Python reads the `{` as a literal because they are not numbers.
fn repeat_bounds(body: &str) -> Option<(&str, Option<&str>)> {
    let (low, high) = match body.split_once(',') {
        Some((low, high)) => (low, Some(high)),
        None => (body, None),
    };
    let is_number = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    if !is_number(low) || !high.is_none_or(is_number) || (high.is_none() && low.is_empty()) {
        return None;
    }
    Some((low, high))
}

/// Python's `min repeat greater than max repeat` error for bounds that `repeat_bounds` accepted.
fn check_bounds(low: &str, high: Option<&str>) -> Result<(), String> {
    let Some(high) = high.filter(|h| !h.is_empty()) else {
        return Ok(());
    };
    let parse = |digits: &str| digits.parse::<u64>().map_err(|e| e.to_string());
    if parse(high)? < parse(low)? {
        return Err("min repeat greater than max repeat".into());
    }
    Ok(())
}

/// The group opening at `chars[i]`.
fn group_open(chars: &[char], i: usize) -> Result<(String, usize), String> {
    if chars.get(i + 1) != Some(&'?') {
        return Ok(("(".into(), i + 1));
    }
    let rest: String = chars[i + 2..].iter().collect();
    if rest.starts_with(':') {
        return Ok(("(?:".into(), i + 3));
    }
    if let Some(named) = rest.strip_prefix("P<") {
        return named_group(named, i);
    }
    if rest.starts_with('#') {
        return inline_comment(chars, i);
    }
    flag_group(chars, i, &rest)
}

/// `(?P<name>` at `chars[i]`, `named` being the text after `(?P<`.
fn named_group(named: &str, i: usize) -> Result<(String, usize), String> {
    let name: String = named.chars().take_while(|&c| c != '>').collect();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(format!("bad group name {name:?}"));
    }
    Ok((format!("(?P<{name}>"), i + 4 + name.chars().count() + 1))
}

/// `(?#...)` at `chars[i]`: no text, and the index after its `)`.
fn inline_comment(chars: &[char], i: usize) -> Result<(String, usize), String> {
    let end = chars[i..]
        .iter()
        .position(|&c| c == ')')
        .ok_or("missing ), unterminated comment")?;
    Ok((String::new(), i + end + 1))
}

/// `(?flags)` or `(?flags:` at `chars[i]`, `rest` being the text after `(?`; anything else after `(?` is
/// lookaround, a conditional or a backreference.
fn flag_group(chars: &[char], i: usize, rest: &str) -> Result<(String, usize), String> {
    let flags: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    let after = chars.get(i + 2 + flags.len());
    if flags.is_empty() || !matches!(after, Some(')') | Some(':')) {
        return Err(format!(
            "(?{rest_head}: lookaround, conditionals and backreferences are not supported",
            rest_head = rest.chars().take(2).collect::<String>()
        ));
    }
    // Python's IGNORECASE folds by `lower()` (`İ` matches `i`), `regex` by Unicode simple case folding.
    if let Some(bad) = flags.chars().find(|f| !matches!(f, 'm' | 's' | 'u')) {
        return Err(format!("flag {bad:?} is not supported"));
    }
    let kept: String = flags.chars().filter(|&f| f != 'u').collect();
    if after == Some(&')') && i != 0 {
        return Err("global flags not at the start of the expression".into());
    }
    // `(?i)` is a whole group in Python; in `regex` it opens no group, so skip its `)`.
    Ok((flag_opening(&kept, after), i + 2 + flags.len() + 1))
}

/// The opening of a flag group in `regex` syntax, `after` being the `)` or `:` that ends the flags.
fn flag_opening(kept: &str, after: Option<&char>) -> String {
    match (kept.is_empty(), after) {
        (true, Some(')')) => String::new(),
        (true, _) => "(?:".into(),
        (false, Some(')')) => format!("(?{kept})"),
        (false, _) => format!("(?{kept}:"),
    }
}

/// The character class at `chars[i]` (`[`) and the index after its `]`.
fn class(chars: &[char], i: usize) -> Result<(String, usize), String> {
    let mut out = String::from("[");
    let mut j = i + 1;
    if chars.get(j) == Some(&'^') {
        out.push('^');
        j += 1;
    }
    let start = j;
    loop {
        let c = *chars.get(j).ok_or("unterminated character set")?;
        if c == ']' && j > start {
            out.push(']');
            return Ok((out, j + 1));
        }
        let (low, next, is_set) = class_item(chars, j)?;
        j = next;
        if starts_range(chars, j) {
            let (range, after) = class_range(chars, j, &low, is_set)?;
            out.push_str(&range);
            j = after;
            continue;
        }
        out.push_str(&low.0);
    }
}

/// A range: `a-z`, unless the `-` at `chars[j]` is last in the set.
fn starts_range(chars: &[char], j: usize) -> bool {
    chars.get(j) == Some(&'-') && chars.get(j + 1).is_some_and(|&c| c != ']')
}

/// The range from `low` (a member of the class, `low_is_set` if it is a set like `\d`) to the member after
/// the `-` at `chars[j]`, and the index after it.
fn class_range(
    chars: &[char],
    j: usize,
    low: &(String, u32),
    low_is_set: bool,
) -> Result<(String, usize), String> {
    let (high, after, high_is_set) = class_item(chars, j + 1)?;
    if low_is_set || high_is_set || low.1 > high.1 {
        return Err("bad character range".into());
    }
    Ok((format!("{}-{}", low.0, high.0), after))
}

/// One class member at `chars[j]`: its text, the code point it stands for (0 for a set like `\d`), the
/// index after it, and whether it is a set.
fn class_item(chars: &[char], j: usize) -> Result<((String, u32), usize, bool), String> {
    let c = chars[j];
    if c != '\\' {
        let mut text = String::new();
        push_class_literal(&mut text, c);
        return Ok(((text, c as u32), j + 1, false));
    }
    let next = chars
        .get(j + 1)
        .copied()
        .ok_or("bad escape (end of pattern)")?;
    if matches!(next, 'd' | 'D' | 'w' | 'W' | 's' | 'S') {
        return Ok(((class_escape(next).to_owned(), 0), j + 2, true));
    }
    let (text, after) = escape(chars, j, Context::Class)?;
    let code = match next {
        'a' => 7,
        'b' => 8,
        'f' => 12,
        'n' => 10,
        'r' => 13,
        't' => 9,
        'v' => 11,
        'x' | 'u' | 'U' => {
            let digits: String = chars[j + 2..after].iter().collect();
            u32::from_str_radix(&digits, 16).map_err(|e| e.to_string())?
        }
        other => other as u32,
    };
    Ok(((text, code), after, false))
}

fn push_class_literal(out: &mut String, c: char) {
    if matches!(c, '[' | ']' | '\\' | '^' | '-' | '&' | '~') {
        out.push('\\');
    }
    out.push(c);
}
