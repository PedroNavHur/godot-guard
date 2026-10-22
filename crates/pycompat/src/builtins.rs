//! Python's built-ins where the ports' output depends on their exact behavior: `repr`, `len`, slicing, and
//! reading a file with `open`.

use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

use crate::string::isprintable;

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
        _ if isprintable(c) => quoted.push(c),
        _ if (c as u32) < 0x100 => quoted.push_str(&format!("\\x{:02x}", c as u32)),
        _ if (c as u32) < 0x10000 => quoted.push_str(&format!("\\u{:04x}", c as u32)),
        _ => quoted.push_str(&format!("\\U{:08x}", c as u32)),
    }
}

/// `len(s)`: code points, not bytes.
pub fn len(s: &str) -> i64 {
    s.chars().count() as i64
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

/// Why `read_text` gave no text: Python raises `OSError` for the first and `UnicodeDecodeError` for the second.
#[derive(Debug)]
pub enum ReadError {
    Io(io::Error),
    Utf8(FromUtf8Error),
}

impl From<io::Error> for ReadError {
    fn from(error: io::Error) -> Self {
        ReadError::Io(error)
    }
}

/// `file_path` under `cwd`; the empty path stays empty, which no file answers to, as `open("")` fails.
pub fn path_under(cwd: &Path, file_path: &str) -> PathBuf {
    match file_path.is_empty() {
        true => PathBuf::new(),
        false => cwd.join(file_path),
    }
}

/// `open(path, encoding="utf-8").read()`: UTF-8 with universal newlines.
pub fn read_text(path: &Path) -> Result<String, ReadError> {
    read_utf8(OpenOptions::new().read(true), path)
}

/// `open(path, "r+", encoding="utf-8").read()`: as `read_text`, from a file also opened for writing.
pub fn read_text_for_update(path: &Path) -> Result<String, ReadError> {
    read_utf8(OpenOptions::new().read(true).write(true), path)
}

fn read_utf8(options: &OpenOptions, path: &Path) -> Result<String, ReadError> {
    let mut bytes = Vec::new();
    options.open(path)?.read_to_end(&mut bytes)?;
    let text = String::from_utf8(bytes).map_err(ReadError::Utf8)?;
    Ok(match text.contains('\r') {
        true => universal_newlines(&text),
        false => text,
    })
}

/// `Path.read_text(encoding="utf-8", errors="replace")`: invalid UTF-8 becomes U+FFFD, and the newlines
/// are universal.
pub fn read_text_replace(path: &Path) -> io::Result<String> {
    Ok(decode_replace(&fs::read(path)?))
}

/// Text-mode decoding of `bytes` with `errors="replace"`, as `read_text_replace`.
pub fn decode_replace(bytes: &[u8]) -> String {
    universal_newlines(&String::from_utf8_lossy(bytes))
}

/// Universal newlines: `\r\n` and a lone `\r` read as `\n`, as Python's text mode reads them.
pub fn universal_newlines(text: &str) -> String {
    match text.contains('\r') {
        true => text.replace("\r\n", "\n").replace('\r', "\n"),
        false => text.to_owned(),
    }
}

fn non_finite_repr(number: f64) -> &'static str {
    match number {
        n if n.is_nan() => "nan",
        n if n > 0.0 => "inf",
        _ => "-inf",
    }
}

/// `repr(float)`: the shortest round-trip digits, in exponent form below 1e-4 and from 1e16.
pub fn float_repr(number: f64) -> String {
    if !number.is_finite() {
        return non_finite_repr(number).into();
    }
    let magnitude = number.abs();
    if magnitude != 0.0 && !(1e-4..1e16).contains(&magnitude) {
        let text = format!("{number:e}");
        let (mantissa, exponent) = text.split_once('e').expect("exponent form");
        let exponent: i32 = exponent.parse().expect("an exponent");
        return format!(
            "{mantissa}e{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.abs()
        );
    }
    let text = format!("{number}");
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_repr_matches_python() {
        assert_eq!(float_repr(1e-5), "1e-05");
        assert_eq!(float_repr(0.0001), "0.0001");
        assert_eq!(float_repr(1e16), "1e+16");
        assert_eq!(float_repr(1e15), "1000000000000000.0");
        assert_eq!(float_repr(1.5e300), "1.5e+300");
        assert_eq!(float_repr(-2.0), "-2.0");
        assert_eq!(float_repr(f64::NEG_INFINITY), "-inf");
    }

    #[test]
    fn repr_matches_python() {
        assert_eq!(repr("abc"), "'abc'");
        assert_eq!(repr("a.gd::gone"), "'a.gd::gone'");
        assert_eq!(repr("it's"), "\"it's\"");
        assert_eq!(repr("'\""), "'\\'\"'");
        assert_eq!(repr("both ' and \""), "'both \\' and \"'");
        assert_eq!(repr("a\\b\n\u{1}"), "'a\\\\b\\n\\x01'");
        assert_eq!(repr("é"), "'é'");
        assert_eq!(
            repr("tab\there\\\x01\u{a0}é"),
            "'tab\\there\\\\\\x01\\xa0é'"
        );
        assert_eq!(repr("\u{2028}\u{1f600}"), "'\\u2028😀'");
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
    fn len_counts_code_points() {
        assert_eq!(len("aé😀"), 3);
    }

    #[test]
    fn newlines_are_universal() {
        assert_eq!(universal_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(decode_replace(b"a\r\nb\rc\n\xff"), "a\nb\nc\n\u{fffd}");
    }

    #[test]
    fn the_empty_path_names_no_file() {
        let cwd = std::env::current_dir().unwrap();
        let error = read_text(&path_under(&cwd, "")).unwrap_err();
        let ReadError::Io(error) = error else {
            panic!("expected an OS error, got {error:?}");
        };
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn invalid_utf8_is_not_an_os_error() {
        let path = std::env::temp_dir().join(format!("pycompat-latin1-{}.gd", std::process::id()));
        fs::write(&path, b"var x = \xe9\n").unwrap();
        let read = read_text(&path);
        let replaced = read_text_replace(&path);
        fs::remove_file(&path).unwrap();
        assert!(matches!(read, Err(ReadError::Utf8(_))), "{read:?}");
        assert_eq!(replaced.unwrap(), "var x = \u{fffd}\n");
    }
}
