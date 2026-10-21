//! gdformat's command line (`gdtoolkit/formatter/__main__.py`), minus `--diff`.

use std::fmt::Display;
use std::io::{Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use crate::args::{Arguments, USAGE, parse_arguments};
use crate::config::{CONFIG_FILE_NAME, DEFAULT_CONFIG_YAML, find_config_file, load_config};
use crate::files::{find_gd_files_from_paths, py_repr, read_text, strerror};
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
    /// The formatted code; the given code when formatting failed.
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

/// gdformat's `main`, run in `cwd`; returns the exit status.
pub fn run(args: &[String], cwd: &Path, io: &mut Io) -> i32 {
    let arguments = match parse_arguments(args) {
        Ok(arguments) => arguments,
        Err(exit) => return exit.print(io),
    };
    if arguments.dump_default_config {
        let path = cwd.join(CONFIG_FILE_NAME);
        if path.is_file() {
            let _ = writeln!(
                io.stderr,
                "AssertionError: {CONFIG_FILE_NAME} already exists"
            );
            return 1;
        }
        return match std::fs::write(&path, DEFAULT_CONFIG_YAML) {
            Ok(()) => 0,
            Err(error) => {
                let _ = writeln!(io.stderr, "{}: {error}", path.display());
                1
            }
        };
    }
    if arguments.diff {
        let _ = writeln!(io.stderr, "gdformat: --diff is not supported by this port");
        return 2;
    }
    let config = match load_config(find_config_file(cwd).as_deref()) {
        Ok(config) => config,
        Err(error) => {
            let _ = writeln!(io.stderr, "gdformat: cannot read the config: {error}");
            return 1;
        }
    };
    let options = Options {
        line_length: arguments.line_length.unwrap_or(config.line_length),
        spaces_for_indent: arguments.use_spaces.or(config.use_spaces),
        safety_checks: !arguments.fast && config.safety_checks,
    };
    let files = find_gd_files_from_paths(&arguments.paths, &config.excluded_directories, cwd);
    let outcome = catch_unwind(AssertUnwindSafe(|| match files.as_slice() {
        [only] if only == "-" => format_stdin(&options, io),
        _ => format_files(&files, &arguments, &options, cwd, io),
    }));
    outcome.unwrap_or_else(|panic| {
        let message = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_default();
        let _ = writeln!(io.stderr, "Traceback: gdformat crashed: {message}");
        1
    })
}

fn format_stdin(options: &Options, io: &mut Io) -> i32 {
    let mut bytes = Vec::new();
    if let Err(error) = io.stdin.read_to_end(&mut bytes) {
        let _ = writeln!(io.stderr, "gdformat: cannot read STDIN: {error}");
        return 1;
    }
    let code = crate::files::universal_newlines(&String::from_utf8_lossy(&bytes));
    let outcome = format_file(&code, options, "STDIN");
    if let Some(message) = &outcome.error_message {
        let _ = io.stderr.write_all(message.as_bytes());
    }
    if !outcome.success {
        return 1;
    }
    let _ = io.stdout.write_all(outcome.formatted_code.as_bytes());
    0
}

fn format_files(
    files: &[String],
    arguments: &Arguments,
    options: &Options,
    cwd: &Path,
    io: &mut Io,
) -> i32 {
    let (mut formatted, mut failed) = (0, 0);
    for file_path in files {
        let path = cwd.join(file_path);
        let code = match read_text(&path, !arguments.check) {
            Ok(code) => code,
            Err(error) => {
                let _ = writeln!(
                    io.stderr,
                    "Cannot open file {}: {}",
                    py_repr(file_path),
                    strerror(&error)
                );
                failed += 1;
                continue;
            }
        };
        let outcome = format_file(&code, options, file_path);
        if let Some(message) = &outcome.error_message {
            let _ = io.stderr.write_all(message.as_bytes());
        }
        if !outcome.success {
            failed += 1;
        } else if outcome.actually_formatted {
            formatted += 1;
            if arguments.check {
                let _ = writeln!(io.stderr, "would reformat {file_path}");
            } else {
                let _ = writeln!(io.stdout, "reformatted {file_path}");
                if let Err(error) = std::fs::write(&path, &outcome.formatted_code) {
                    let _ = writeln!(
                        io.stderr,
                        "Cannot open file {}: {}",
                        py_repr(file_path),
                        strerror(&error)
                    );
                    failed += 1;
                }
            }
        }
    }
    let summary = match arguments.check {
        true => check_summary(files.len(), formatted, failed),
        false => format_summary(files.len(), formatted, failed),
    };
    let _ = io.stdout.write_all(summary.stdout.as_bytes());
    let _ = io.stderr.write_all(summary.stderr.as_bytes());
    summary.exit_status
}

/// `parse_arguments` failed: docopt prints usage and exits 1, or prints help or the version and exits 0.
pub enum ArgumentsExit {
    Usage(String),
    Help,
    Version,
}

impl ArgumentsExit {
    fn print(&self, io: &mut Io) -> i32 {
        match self {
            ArgumentsExit::Usage(warning) => {
                let _ = writeln!(io.stderr, "{warning}{USAGE}");
                1
            }
            ArgumentsExit::Help => {
                let _ = writeln!(io.stdout, "{}", crate::args::HELP);
                0
            }
            ArgumentsExit::Version => {
                let _ = writeln!(io.stdout, "gdformat 4.5.0");
                0
            }
        }
    }
}
