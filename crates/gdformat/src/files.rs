//! Finding, reading, and naming files as gdformat's Python does.

use std::fs::OpenOptions;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::string::FromUtf8Error;

/// `find_gd_files_from_paths`: each path that is a directory, walked as `os.walk` walks it, top-down, in
/// directory-listing order, not into symlinked or excluded directories; any other path as given.
pub fn find_gd_files_from_paths(
    paths: &[String],
    excluded_directories: &[String],
    cwd: &Path,
) -> Vec<String> {
    let mut files = Vec::new();
    for path in paths {
        if !path.is_empty() && cwd.join(path).is_dir() {
            walk(path, excluded_directories, cwd, &mut files);
        } else {
            files.push(path.clone());
        }
    }
    files
}

fn walk(top: &str, excluded_directories: &[String], cwd: &Path, files: &mut Vec<String>) {
    let Some((dirnames, gd_files)) = scandir(top, cwd) else {
        return;
    };
    files.extend(gd_files);
    for dirname in dirnames
        .iter()
        .filter(|d| !excluded_directories.contains(d))
    {
        let new_path = join(top, dirname);
        if !is_symlink(&cwd.join(&new_path)) {
            walk(&new_path, excluded_directories, cwd, files);
        }
    }
}

/// The subdirectory names of `top` and its `.gd` files as paths from `top`; None when it cannot be listed,
/// as `os.walk` without `onerror` skips it.
fn scandir(top: &str, cwd: &Path) -> Option<(Vec<String>, Vec<String>)> {
    let entries = std::fs::read_dir(cwd.join(top)).ok()?;
    let mut dirnames = Vec::new();
    let mut gd_files = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `DirEntry.is_dir()` follows symlinks, as `os.scandir`'s does.
        if entry.path().is_dir() {
            dirnames.push(name);
        } else if name.ends_with(".gd") {
            gd_files.push(join(top, &name));
        }
    }
    Some((dirnames, gd_files))
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// `os.path.join(a, b)` for a relative `b`.
fn join(a: &str, b: &str) -> String {
    match a.is_empty() || a.ends_with('/') {
        true => format!("{a}{b}"),
        false => format!("{a}/{b}"),
    }
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

/// `open(path, encoding="utf-8").read()`, or `"r+"` when `writable`: UTF-8 with universal newlines.
pub fn read_text(path: &Path, writable: bool) -> Result<String, ReadError> {
    let mut file = OpenOptions::new().read(true).write(writable).open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let text = String::from_utf8(bytes).map_err(ReadError::Utf8)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_like_os_path() {
        assert_eq!(join("scripts", "a.gd"), "scripts/a.gd");
        assert_eq!(join("scripts/", "a.gd"), "scripts/a.gd");
    }

    #[test]
    fn reads_newlines_like_text_mode() {
        assert_eq!(universal_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn the_empty_path_is_a_file_not_the_working_directory() {
        let cwd = std::env::current_dir().unwrap();
        let found = find_gd_files_from_paths(&[String::new()], &[], &cwd);
        assert_eq!(found, [""]);
        let error = read_text(&path_under(&cwd, ""), false).unwrap_err();
        let ReadError::Io(error) = error else {
            panic!("expected an OS error, got {error:?}");
        };
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn invalid_utf8_is_not_an_os_error() {
        let path = std::env::temp_dir().join(format!("gdformat-latin1-{}.gd", std::process::id()));
        std::fs::write(&path, b"var x = \xe9\n").unwrap();
        let read = read_text(&path, false);
        std::fs::remove_file(&path).unwrap();
        assert!(matches!(read, Err(ReadError::Utf8(_))), "{read:?}");
    }
}
