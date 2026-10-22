//! gdformat and gdlint, run in-process where 0.4.0 runs gdtoolkit's commands.
//!
//! Each takes the command's arguments and runs in `root`, as 0.4.0's child did. With an echo, the command's
//! stdout and stderr merge into it in the order it writes them, as the pipe 0.4.0 reads merges them;
//! without, they go to the terminal's streams.

use std::cell::RefCell;
use std::io::{self, Write};
use std::path::Path;

use crate::error::{Failure, Result};
use crate::tools::{self, Echo};

/// One of two writers onto a shared buffer.
struct Merged<'a>(&'a RefCell<Vec<u8>>);

impl Write for Merged<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A closed pipe is not a crash: the reader went away, so the command's status is 1, as gdformat's
/// binary exits, and `tools::write_stdout` drops the same error.
fn excuse_closed_pipe(result: io::Result<i32>) -> io::Result<i32> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(1),
        other => other,
    }
}

/// Run `command` on (stdout, stderr), merged into `echo` line by line when there is one. A write it
/// fails, or the terminal's final flush failing, is the error, except for a closed pipe, which
/// is status 1 without a crash message.
fn streams(
    echo: Option<Echo>,
    command: impl FnOnce(&mut dyn Write, &mut dyn Write) -> io::Result<i32>,
) -> io::Result<i32> {
    let Some(echo) = echo else {
        let (mut out, mut err) = (io::stdout().lock(), io::stderr().lock());
        let result = excuse_closed_pipe(command(&mut out, &mut err));
        return match out.flush() {
            Err(error) if error.kind() != io::ErrorKind::BrokenPipe => Err(error),
            _ => result,
        };
    };
    let buffer = RefCell::new(Vec::new());
    let result = command(&mut Merged(&buffer), &mut Merged(&buffer));
    for line in tools::read_lines(&buffer.borrow()[..]) {
        echo(&line);
    }
    result
}

/// `gdlint ARGS` in `root`.
pub fn gdlint(args: &[String], root: &Path, echo: Option<Echo>) -> Result<i32> {
    streams(echo, |out, err| {
        gdlint::cli::run(args, root, &gdscript::parse, out, err)
    })
    .map_err(|e| Failure::crash("writing gdlint's output", e))
}

/// `gdformat ARGS` in `root`; it reads no stdin when captured.
pub fn gdformat(args: &[String], root: &Path, echo: Option<Echo>) -> Result<i32> {
    let captured = echo.is_some();
    streams(echo, |stdout, stderr| {
        let (mut piped, mut terminal) = (io::empty(), io::stdin());
        let stdin: &mut dyn io::Read = if captured { &mut piped } else { &mut terminal };
        gdformat::cli::run(
            args,
            root,
            &mut gdformat::cli::Io {
                stdin,
                stdout,
                stderr,
            },
        )
    })
    .map_err(|e| Failure::crash("writing gdformat's output", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_pipe_is_status_1_not_an_error() {
        let result = excuse_closed_pipe(Err(io::ErrorKind::BrokenPipe.into()));
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn other_errors_and_statuses_pass_through() {
        let denied = excuse_closed_pipe(Err(io::ErrorKind::PermissionDenied.into()));
        assert_eq!(denied.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(excuse_closed_pipe(Ok(2)).unwrap(), 2);
    }
}
