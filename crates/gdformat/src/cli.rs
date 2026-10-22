//! gdformat's command line (`gdtoolkit/formatter/__main__.py`), minus `--diff`.

use std::any::Any;
use std::fmt::Display;
use std::io::{self, Read, Write};
use std::ops::AddAssign;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use gdlint::repr::str_repr;

use crate::args::{Arguments, USAGE, parse_arguments};
use crate::config::{CONFIG_FILE_NAME, Config, DEFAULT_CONFIG_YAML, find_config_file, load_config};
use crate::files::{ReadError, find_gd_files_from_paths, path_under, read_text, strerror};
use crate::formatter::format_code;
use crate::parse::parse_code;
use crate::safety_checks::{ParsedCode, SafetyError, SafetyViolation, check_formatting_safety};

/// The options that change what gdformat writes, resolved from the command line and the config.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub line_length: i64,
    /// None indents with tabs.
    pub spaces_for_indent: Option<usize>,
    pub safety_checks: bool,
}

/// `_format_code`'s result for one file, and what it printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFormatting {
    pub success: bool,
    pub actually_formatted: bool,
    /// The formatted code; the given code when parsing failed, the unsafe output when a safety check
    /// failed.
    pub formatted_code: String,
    /// What gdformat prints to stderr for this file when it fails, newline included.
    pub error_message: Option<String>,
}

/// `_format_code`: format `code`, whose trees are `given` (or the error parsing it raised), and run the
/// safety checks when asked, parsing the formatted code with `parse`.
pub fn format_file_code<G: Display, E: Display>(
    code: &str,
    options: &Options,
    file_path: &str,
    given: Result<&ParsedCode, G>,
    parse: impl FnOnce(&str) -> Result<ParsedCode, E>,
) -> FileFormatting {
    let failed = |message: String| FileFormatting {
        success: false,
        actually_formatted: false,
        formatted_code: code.to_owned(),
        error_message: Some(message),
    };
    let given = match given {
        Ok(given) => given,
        Err(error) => return failed(format!("{file_path}:\n\n{error}\n")),
    };
    let formatted_code = format_code(
        code,
        options.line_length,
        options.spaces_for_indent,
        &given.tree,
        &given.comments,
    );
    let actually_formatted = formatted_code != code;
    let mut outcome = FileFormatting {
        success: true,
        actually_formatted,
        formatted_code,
        error_message: None,
    };
    if !(actually_formatted && options.safety_checks) {
        return outcome;
    }
    let verdict = check_formatting_safety(
        code,
        &outcome.formatted_code,
        options.line_length,
        given,
        options.spaces_for_indent,
        parse,
    );
    let Err(error) = verdict else {
        return outcome;
    };
    outcome.success = false;
    outcome.error_message = Some(match error {
        SafetyError::Parse(error) => format!("{file_path}:\n\n{error}\n"),
        SafetyError::Violation(SafetyViolation::TreeInvariantViolation) => {
            format!("{file_path}: Failed to format, formatted code parse tree differs\n")
        }
        SafetyError::Violation(SafetyViolation::FormattingStabilityViolation) => {
            format!("{file_path}: Failed to format, formatted code is unstable\n")
        }
        SafetyError::Violation(SafetyViolation::CommentPersistenceViolation { .. }) => {
            format!("{file_path}: Failed to format,some comments are missing in formatted code\n")
        }
    });
    outcome
}

/// `_format_code` on a file's text, parsed with `gdscript`: what gdformat does for each file, without
/// printing. `code` must be read as Python reads text files, as `read_text` reads them.
pub fn format_file(code: &str, options: &Options, file_path: &str) -> FileFormatting {
    format_file_code(
        code,
        options,
        file_path,
        parse_code(code).as_ref(),
        parse_code,
    )
}

/// `"s"` unless there is exactly one.
fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

/// What `--check` prints after the files, and its exit status.
pub fn check_summary(files: usize, formattable: usize, failed: usize) -> Summary {
    if formattable == 0 {
        return Summary {
            stdout: format!("{files} file{} would be left unchanged\n", plural(files)),
            stderr: String::new(),
            exit_status: i32::from(failed != 0),
        };
    }
    let left_unchanged = files - formattable;
    Summary {
        stdout: String::new(),
        stderr: format!(
            "{formattable} file{} would be reformatted, {left_unchanged} file{} would be left unchanged.\n",
            plural(formattable),
            plural(left_unchanged)
        ),
        exit_status: 1,
    }
}

/// What formatting in place prints after the files, and its exit status.
pub fn format_summary(files: usize, reformatted: usize, failed: usize) -> Summary {
    let left_unchanged = files - reformatted;
    Summary {
        stdout: format!(
            "{reformatted} file{} reformatted, {left_unchanged} file{} left unchanged.\n",
            plural(reformatted),
            plural(left_unchanged)
        ),
        stderr: String::new(),
        exit_status: i32::from(failed != 0),
    }
}

pub struct Summary {
    pub stdout: String,
    pub stderr: String,
    pub exit_status: i32,
}

pub struct Io<'a> {
    pub stdin: &'a mut dyn Read,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
}

/// gdformat's `main`, run in `cwd`; returns the exit status, or the error writing to stdout. Writes to
/// stderr are best-effort.
pub fn run(args: &[String], cwd: &Path, io: &mut Io) -> io::Result<i32> {
    let arguments = match parse_arguments(args) {
        Ok(arguments) => arguments,
        Err(exit) => return exit.print(io),
    };
    if arguments.dump_default_config {
        return Ok(dump_default_config(cwd, io));
    }
    if arguments.diff {
        let _ = writeln!(io.stderr, "gdformat: --diff is not supported by this port");
        return Ok(2);
    }
    let config = match load_config(find_config_file(cwd).as_deref()) {
        Ok(config) => config,
        Err(error) => {
            let _ = writeln!(io.stderr, "gdformat: cannot read the config: {error}");
            return Ok(1);
        }
    };
    let options = resolve_options(&arguments, &config);
    let files = find_gd_files_from_paths(&arguments.paths, &config.excluded_directories, cwd);
    let outcome = catch_unwind(AssertUnwindSafe(|| match files.as_slice() {
        [only] if only == "-" => format_stdin(&options, io),
        _ => format_files(&files, &arguments, &options, cwd, io),
    }));
    let status = outcome.unwrap_or_else(|panic| Ok(report_crash(&*panic, io)))?;
    io.stdout.flush()?;
    Ok(status)
}

/// `--dump-default-config`: write `gdformatrc` in `cwd`; returns the exit status.
fn dump_default_config(cwd: &Path, io: &mut Io) -> i32 {
    let path = cwd.join(CONFIG_FILE_NAME);
    if path.is_file() {
        let _ = writeln!(
            io.stderr,
            "AssertionError: {CONFIG_FILE_NAME} already exists"
        );
        return 1;
    }
    match std::fs::write(&path, DEFAULT_CONFIG_YAML) {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(io.stderr, "{}: {error}", path.display());
            1
        }
    }
}

/// The command line over the config, for each option.
fn resolve_options(arguments: &Arguments, config: &Config) -> Options {
    Options {
        line_length: arguments.line_length.unwrap_or(config.line_length),
        spaces_for_indent: arguments.use_spaces.or(config.use_spaces),
        safety_checks: !arguments.fast && config.safety_checks,
    }
}

/// A panic while formatting is gdtoolkit's uncaught exception: say so, and exit 1.
fn report_crash(panic: &(dyn Any + Send), io: &mut Io) -> i32 {
    let message = panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
        .unwrap_or_default();
    let _ = writeln!(io.stderr, "Traceback: gdformat crashed: {message}");
    1
}

fn format_stdin(options: &Options, io: &mut Io) -> io::Result<i32> {
    let mut bytes = Vec::new();
    if let Err(error) = io.stdin.read_to_end(&mut bytes) {
        let _ = writeln!(io.stderr, "gdformat: cannot read STDIN: {error}");
        return Ok(1);
    }
    let code = crate::files::universal_newlines(&String::from_utf8_lossy(&bytes));
    let outcome = format_file(&code, options, "STDIN");
    if let Some(message) = &outcome.error_message {
        let _ = io.stderr.write_all(message.as_bytes());
    }
    if !outcome.success {
        return Ok(1);
    }
    io.stdout.write_all(outcome.formatted_code.as_bytes())?;
    Ok(0)
}

/// What one file adds to the summary: the files in `formatted_files` and in `failed_files`.
#[derive(Clone, Copy, Default)]
struct Counts {
    formatted: usize,
    failed: usize,
}

impl AddAssign for Counts {
    fn add_assign(&mut self, other: Counts) {
        self.formatted += other.formatted;
        self.failed += other.failed;
    }
}

const FORMATTED: Counts = Counts {
    formatted: 1,
    failed: 0,
};
const FAILED: Counts = Counts {
    formatted: 0,
    failed: 1,
};

/// Writes to stdout as Python's block-buffered one fails: the first error is kept and returned at the
/// end, after every file is formatted, and nothing is written once it has failed.
#[derive(Default)]
struct DeferredStdout {
    error: Option<io::Error>,
}

impl DeferredStdout {
    fn write(&mut self, stdout: &mut dyn Write, text: &str) {
        if self.error.is_some() {
            return;
        }
        self.error = stdout.write_all(text.as_bytes()).err();
    }

    fn finish(self, status: i32) -> io::Result<i32> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(status),
        }
    }
}

fn format_files(
    files: &[String],
    arguments: &Arguments,
    options: &Options,
    cwd: &Path,
    io: &mut Io,
) -> io::Result<i32> {
    let mut total = Counts::default();
    let mut stdout = DeferredStdout::default();
    for file_path in files {
        let Some(counts) = format_one_file(file_path, arguments, options, cwd, io) else {
            return stdout.finish(1);
        };
        if counts.formatted > 0 && !arguments.check {
            stdout.write(io.stdout, &format!("reformatted {file_path}\n"));
        }
        total += counts;
    }
    let summary = match arguments.check {
        true => check_summary(files.len(), total.formatted, total.failed),
        false => format_summary(files.len(), total.formatted, total.failed),
    };
    stdout.write(io.stdout, &summary.stdout);
    let _ = io.stderr.write_all(summary.stderr.as_bytes());
    stdout.finish(summary.exit_status)
}

/// `_check_files_formatting` and `_format_files` for one file, writing the formatted code back unless
/// checking: its share of the counts, or None when gdformat would stop with a traceback.
fn format_one_file(
    file_path: &str,
    arguments: &Arguments,
    options: &Options,
    cwd: &Path,
    io: &mut Io,
) -> Option<Counts> {
    let path = path_under(cwd, file_path);
    let code = match read_text(&path, !arguments.check) {
        Ok(code) => code,
        Err(ReadError::Io(error)) => {
            report_cannot_open(file_path, &error, io);
            return Some(FAILED);
        }
        Err(ReadError::Utf8(error)) => {
            let _ = writeln!(
                io.stderr,
                "{file_path}: UnicodeDecodeError: 'utf-8' codec can't decode the file: {error}"
            );
            return None;
        }
    };
    let outcome = format_file(&code, options, file_path);
    if let Some(message) = &outcome.error_message {
        let _ = io.stderr.write_all(message.as_bytes());
    }
    if !outcome.success {
        return Some(FAILED);
    }
    if !outcome.actually_formatted {
        return Some(Counts::default());
    }
    if arguments.check {
        let _ = writeln!(io.stderr, "would reformat {file_path}");
        return Some(FORMATTED);
    }
    let mut counts = FORMATTED;
    if let Err(error) = std::fs::write(&path, &outcome.formatted_code) {
        report_cannot_open(file_path, &error, io);
        counts += FAILED;
    }
    Some(counts)
}

fn report_cannot_open(file_path: &str, error: &io::Error, io: &mut Io) {
    let _ = writeln!(
        io.stderr,
        "Cannot open file {}: {}",
        str_repr(file_path),
        strerror(error)
    );
}

/// `parse_arguments` failed: docopt prints usage and exits 1, or prints help or the version and exits 0.
pub enum ArgumentsExit {
    Usage(String),
    Help,
    Version,
}

impl ArgumentsExit {
    fn print(&self, io: &mut Io) -> io::Result<i32> {
        match self {
            ArgumentsExit::Usage(warning) => {
                let _ = writeln!(io.stderr, "{warning}{USAGE}");
                Ok(1)
            }
            ArgumentsExit::Help => {
                writeln!(io.stdout, "{}", crate::args::HELP)?;
                Ok(0)
            }
            ArgumentsExit::Version => {
                writeln!(io.stdout, "gdformat 4.5.0")?;
                Ok(0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ClosedPipe;

    impl Write for ClosedPipe {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn run_with(args: &[&str], cwd: &Path, stdin: &[u8], stdout: &mut dyn Write) -> (String, i32) {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        let mut stderr = Vec::new();
        let mut stdin = stdin;
        let status = run(
            &args,
            cwd,
            &mut Io {
                stdin: &mut stdin,
                stdout,
                stderr: &mut stderr,
            },
        )
        .unwrap_or(-1);
        (String::from_utf8(stderr).unwrap(), status)
    }

    #[test]
    fn a_script_that_is_not_utf8_stops_the_run() {
        let dir = std::env::temp_dir().join(format!("gdformat-utf8-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.gd"), b"var x = \xe9\n").unwrap();
        std::fs::write(dir.join("b.gd"), "func f(a,b):\n\treturn a+b\n").unwrap();
        let (stderr, status) = run_with(&["a.gd", "b.gd"], &dir, b"", &mut Vec::new());
        let untouched = std::fs::read_to_string(dir.join("b.gd")).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(status, 1);
        assert!(stderr.starts_with("a.gd: UnicodeDecodeError:"), "{stderr}");
        assert_eq!(untouched, "func f(a,b):\n\treturn a+b\n");
    }

    #[test]
    fn a_failed_write_to_stdout_is_the_error() {
        let result = run(
            &["-".to_owned()],
            Path::new("/"),
            &mut Io {
                stdin: &mut &b"var x = 1\n"[..],
                stdout: &mut ClosedPipe,
                stderr: &mut Vec::new(),
            },
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn a_closed_stdout_does_not_stop_the_files_being_formatted() {
        let dir = std::env::temp_dir().join(format!("gdformat-pipe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["a.gd", "b.gd"] {
            std::fs::write(dir.join(name), "func f(a,b):\n\treturn a+b\n").unwrap();
        }
        let args = ["a.gd".to_owned(), "b.gd".to_owned()];
        let result = run(
            &args,
            &dir,
            &mut Io {
                stdin: &mut &b""[..],
                stdout: &mut ClosedPipe,
                stderr: &mut Vec::new(),
            },
        );
        let formatted: Vec<String> = ["a.gd", "b.gd"]
            .iter()
            .map(|name| std::fs::read_to_string(dir.join(name)).unwrap())
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(formatted, ["func f(a, b):\n\treturn a + b\n"; 2]);
    }

    #[test]
    fn a_file_name_prints_as_python_prints_it() {
        let (stderr, _) = run_with(&["-c", "a\u{a0}.gd"], Path::new("/"), b"", &mut Vec::new());
        assert!(
            stderr.starts_with("Cannot open file 'a\\xa0.gd': No such file or directory\n"),
            "{stderr}"
        );
    }
}
