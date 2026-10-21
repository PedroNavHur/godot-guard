//! Finding, reading, and naming files as gdformat's Python does.

use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::Path;

/// `find_gd_files_from_paths`: each directory walked top-down for `.gd` files, in the order the
/// filesystem lists them as `os.walk` does, skipping directories named in `excluded_directories`; any
/// other path as given. Paths are relative to `cwd` as given, joined as `os.path.join` joins them.
pub fn find_gd_files_from_paths(
    paths: &[String],
    excluded_directories: &[String],
    cwd: &Path,
) -> Vec<String> {
    let mut files = Vec::new();
    for path in paths {
        if cwd.join(path).is_dir() {
            walk(path, excluded_directories, cwd, &mut files);
        } else {
            files.push(path.clone());
        }
    }
    files
}

fn walk(dir: &str, excluded_directories: &[String], cwd: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(cwd.join(dir)) else {
        return;
    };
    let mut subdirectories = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // os.walk follows symlinks to directories when listing, not when descending.
        let is_dir = entry.path().is_dir();
        if is_dir {
            let descend = !excluded_directories.contains(&name)
                && !entry.file_type().is_ok_and(|kind| kind.is_symlink());
            if descend {
                subdirectories.push(name);
            }
        } else if name.ends_with(".gd") {
            files.push(join(dir, &name));
        }
    }
    for subdirectory in subdirectories {
        walk(&join(dir, &subdirectory), excluded_directories, cwd, files);
    }
}

/// `os.path.join(a, b)` for a relative `b`.
fn join(a: &str, b: &str) -> String {
    match a.is_empty() || a.ends_with('/') {
        true => format!("{a}{b}"),
        false => format!("{a}/{b}"),
    }
}

/// `open(path, encoding="utf-8").read()`, or `"r+"` when `writable`: UTF-8 with universal newlines.
pub fn read_text(path: &Path, writable: bool) -> io::Result<String> {
    let mut file = OpenOptions::new().read(true).write(writable).open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(universal_newlines(&text))
}

/// `\r\n` and lone `\r` read as `\n`, as Python's text mode reads them.
pub fn universal_newlines(text: &str) -> String {
    match text.contains('\r') {
        true => text.replace("\r\n", "\n").replace('\r', "\n"),
        false => text.to_owned(),
    }
}

/// The C library's message for the error, as Python's `OSError.strerror` gives it.
pub fn strerror(error: &io::Error) -> String {
    let text = error.to_string();
    match text.rfind(" (os error ") {
        Some(at) => text[..at].to_owned(),
        None => text,
    }
}

/// `repr(s)` of a Python string.
pub fn py_repr(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr_matches_python() {
        assert_eq!(py_repr("a.gd"), "'a.gd'");
        assert_eq!(py_repr("it's.gd"), "\"it's.gd\"");
        assert_eq!(py_repr("a'\"b"), "'a\\'\"b'");
        assert_eq!(py_repr("tab\t"), "'tab\\t'");
    }

    #[test]
    fn joins_like_os_path() {
        assert_eq!(join("scripts", "a.gd"), "scripts/a.gd");
        assert_eq!(join("scripts/", "a.gd"), "scripts/a.gd");
    }

    #[test]
    fn reads_newlines_like_text_mode() {
        assert_eq!(universal_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
    }
}
