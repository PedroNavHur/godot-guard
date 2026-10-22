//! `os.path.join` and `OSError.strerror`.

use std::io;

/// `os.path.join(a, b)` for a relative `b`.
pub fn join(a: &str, b: &str) -> String {
    match a.is_empty() || a.ends_with('/') {
        true => format!("{a}{b}"),
        false => format!("{a}/{b}"),
    }
}

/// The C library's message for the error, as Python's `OSError.strerror` gives it: without Rust's
/// "(os error N)".
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
        assert_eq!(join("", "a.gd"), "a.gd");
    }

    #[test]
    fn strerror_drops_the_os_error_suffix() {
        let error = io::Error::from_raw_os_error(2);
        assert_eq!(strerror(&error), "No such file or directory");
        assert_eq!(strerror(&io::Error::other("plain")), "plain");
    }
}
