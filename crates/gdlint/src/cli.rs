//! `linter/__main__.py`: the `gdlint` command, with the parser passed in.
//!
//! [`run`] writes what gdlint writes, to the same streams, and returns its exit status. Where the Python
//! dies with a traceback (an unreadable config, a file that is not UTF-8, an indenter error, `-d` over an
//! existing file), this prints a one-line error instead, with the same status, 1.

use std::io::{self, Write};
use std::path::Path;

use gdscript::{ParseError, Tree};
use pycompat::builtins::{ReadError, path_under, read_text, repr};
use pycompat::os::strerror;
use pycompat::utils::find_gd_files_from_paths;
use pycompat::yaml::{Value, value_repr};

use crate::config::{self, CONFIG_FILE_NAME, Config, ConfigError};
use crate::docopt::{self, Command};
use crate::parse_error;
use crate::problem::Printed;

/// A parser, `gdscript::parse` outside tests: the file's tree, or why gdtoolkit cannot parse it.
pub type Parse<'a> = dyn Fn(&str) -> Result<Tree, ParseError> + Sync + 'a;

pub const VERSION: &str = "gdlint 4.5.0";

/// The config gdlint uses when run in `cwd`: the nearest `gdlintrc` or `.gdlintrc`, else the defaults.
pub fn load_config(cwd: &Path) -> Result<Config, ConfigError> {
    load_config_logged(cwd, &mut |_| {})
}

fn load_config_logged(cwd: &Path, log: &mut dyn FnMut(String)) -> Result<Config, ConfigError> {
    let Some(path) = config::find_config_file(cwd) else {
        log("No 'gdlintrc' nor '.gdlintrc' found. Using default config...".into());
        log("Loaded config:".into());
        for (key, value) in config::default_entries() {
            log(format!("({}, {})", repr(key), value_repr(&value)));
        }
        return Ok(Config::defaults());
    };
    let shown = path.display().to_string();
    log(format!("Config file found: '{shown}'"));
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ConfigError(format!("cannot read {shown}: {}", strerror(&e))))?;
    let entries = config::parse_entries(&text).map_err(|e| ConfigError(format!("{shown}: {e}")))?;
    log("Loaded config:".into());
    for (key, value) in &entries {
        log(format!("({}, {})", value_repr(key), value_repr(value)));
    }
    for (key, value) in config::default_entries() {
        if !entries
            .iter()
            .any(|(k, _)| *k == Value::Str(key.to_owned()))
        {
            let entry = format!("({}, {})", repr(key), value_repr(&value));
            log(format!("Adding missing entry from defaults: {entry}"));
        }
    }
    Config::from_entries(config::string_keyed(&entries))
        .map_err(|e| ConfigError(format!("{shown}: {e}")))
}

/// `main()`, for `argv` without the program name, run in `cwd`.
pub fn run(
    argv: &[String],
    cwd: &Path,
    parse: &Parse,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<i32> {
    match docopt::parse(argv) {
        Command::Help => {
            writeln!(out, "{}", docopt::DOC)?;
            Ok(0)
        }
        Command::Version => {
            writeln!(out, "{VERSION}")?;
            Ok(0)
        }
        Command::Usage(message) => {
            writeln!(err, "{message}")?;
            Ok(1)
        }
        Command::Dump => dump_default_config(cwd, err),
        Command::Lint { paths, verbose } => lint_paths(&paths, verbose, cwd, parse, out, err),
    }
}

/// The lint command's work: load the config (logging it to `out` when `verbose`), lint each file, sum up.
fn lint_paths(
    paths: &[String],
    verbose: bool,
    cwd: &Path,
    parse: &Parse,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<i32> {
    let config = match load_config_verbose(cwd, verbose, out)? {
        Ok(config) => config,
        Err(error) => {
            writeln!(err, "gdlint: {error}")?;
            return Ok(1);
        }
    };
    let mut problems_total = 0;
    for file_path in find_gd_files_from_paths(paths, &config.excluded_directories, cwd) {
        match lint_file(&file_path, cwd, &config, parse, err)? {
            Some(count) => problems_total += count,
            None => return Ok(1),
        }
    }
    summary(problems_total, out, err)
}

/// The config, logging to `out` as `--verbose` does; an error is a failure to write the log.
fn load_config_verbose(
    cwd: &Path,
    verbose: bool,
    out: &mut dyn Write,
) -> io::Result<Result<Config, ConfigError>> {
    let mut log_failure = None;
    let mut log = |message: String| {
        if verbose && log_failure.is_none() {
            log_failure = writeln!(out, "INFO:root:{message}").err();
        }
    };
    let config = load_config_logged(cwd, &mut log);
    match log_failure {
        Some(failure) => Err(failure),
        None => Ok(config),
    }
}

/// The closing line, and the exit status it goes with.
fn summary(problems_total: usize, out: &mut dyn Write, err: &mut dyn Write) -> io::Result<i32> {
    if problems_total == 0 {
        writeln!(out, "Success: no problems found")?;
        return Ok(0);
    }
    let plural = if problems_total == 1 { "" } else { "s" };
    writeln!(err, "Failure: {problems_total} problem{plural} found")?;
    Ok(1)
}

fn dump_default_config(cwd: &Path, err: &mut dyn Write) -> io::Result<i32> {
    let path = cwd.join(CONFIG_FILE_NAME);
    if path.is_file() {
        writeln!(err, "AssertionError: {CONFIG_FILE_NAME} already exists")?;
        return Ok(1);
    }
    std::fs::write(&path, config::DEFAULT_CONFIG_DUMP)?;
    Ok(0)
}

/// `_lint_file`: the problems counted for one file, or None when gdlint would stop with a traceback.
fn lint_file(
    file_path: &str,
    cwd: &Path,
    config: &Config,
    parse: &Parse,
    err: &mut dyn Write,
) -> io::Result<Option<usize>> {
    let content = match read_text(&path_under(cwd, file_path)) {
        Ok(content) => content,
        Err(ReadError::Utf8(utf8)) => {
            writeln!(
                err,
                "{file_path}: UnicodeDecodeError: 'utf-8' codec can't decode the file: {utf8}"
            )?;
            return Ok(None);
        }
        Err(ReadError::Io(error)) => {
            writeln!(err, "Cannot open file '{file_path}': {}", strerror(&error))?;
            return Ok(Some(1));
        }
    };
    let tree = match parse(&content) {
        Ok(tree) => tree,
        Err(error) => match parse_error::report(&error, &content) {
            Ok(report) => {
                writeln!(err, "{file_path}:\n\n{report}")?;
                return Ok(Some(1));
            }
            Err(exception) => {
                writeln!(err, "{file_path}: {exception}")?;
                return Ok(None);
            }
        },
    };
    let problems = crate::lint_code(&content, &tree, config);
    for problem in &problems {
        writeln!(err, "{}", Printed { problem, file_path })?;
    }
    Ok(Some(problems.len()))
}
