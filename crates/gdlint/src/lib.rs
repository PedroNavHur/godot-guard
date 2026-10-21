//! gdlint: the same problems as gdtoolkit 4.5.0's linter, read from a `gdscript::Tree`.
//!
//! Library use: load a [`Config`] once ([`Config::defaults`], or [`Config::from_entries`] over a
//! `.gdlintrc`), then call [`lint_code`] per file with the file's text, as [`read_source`] returns it, and its
//! parse tree. [`Problem`]s print with [`Printed`].

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use gdscript::Tree;
use regex::Regex;

pub mod ast;
pub mod config;
mod design_checks;
mod helpers;
mod problem;
pub mod pyre;
pub mod repr;
mod text;
pub mod utils;
pub mod yaml;

pub use config::{Config, ConfigError};
pub use problem::{Printed, Problem};
pub use text::universal_newlines;

/// A file's text as gdlint reads it, `open(path, encoding="utf-8").read()`: UTF-8 with universal newlines.
/// The outer error is the open or read failing; the inner one is text that is not UTF-8.
pub fn read_source(
    path: &std::path::Path,
) -> std::io::Result<Result<String, std::string::FromUtf8Error>> {
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8(bytes).map(universal_newlines))
}

/// `lint_code`: the problems in `gdscript_code`, whose parse tree is `parse_tree`, in gdlint's order.
pub fn lint_code(gdscript_code: &str, parse_tree: &Tree, config: &Config) -> Vec<Problem> {
    let ast = ast::AbstractSyntaxTree::new(parse_tree);
    let mut problems = design_checks::lint(&ast, config);

    let inactive = fetch_problem_inactivity_lines(gdscript_code);
    problems.retain(|problem| {
        inactive
            .get(problem.name)
            .is_none_or(|lines| !lines.contains(&problem.line))
    });
    problems
}

/// Per problem name, the lines where `# gdlint: ignore=` and `# gdlint: disable=`/`enable=` turn it off.
fn fetch_problem_inactivity_lines(gdscript_code: &str) -> HashMap<String, HashSet<usize>> {
    let lines = text::splitlines(gdscript_code);
    let mut problem_inactivity_lines: HashMap<String, HashSet<usize>> = HashMap::new();
    for (line_no, problems) in directives(&lines, Directive::Ignore) {
        for problem in problems {
            let entry = problem_inactivity_lines.entry(problem).or_default();
            entry.insert(line_no);
            entry.insert(line_no + 1);
        }
    }
    for (problem, ranges) in fetch_problem_inactivity_ranges(&lines) {
        let entry = problem_inactivity_lines.entry(problem).or_default();
        for (begin, end) in ranges {
            entry.extend(begin..=end);
        }
    }
    problem_inactivity_lines
}

/// `_fetch_problem_inactivity_ranges`: each disable runs to the end of the file, cut short by the earliest
/// enable of that problem anywhere in the file, before or after it, as the Python does.
fn fetch_problem_inactivity_ranges(lines: &[&str]) -> HashMap<String, Vec<(usize, usize)>> {
    let last_line_no = lines.len();
    let mut ranges: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    for (line_no, problems) in directives(lines, Directive::Disable) {
        // A directive after code disables from the next line; one on its own line, from that line.
        let disable_from_next_line = !text::python_strip(lines[line_no - 1]).starts_with('#');
        let begin = line_no + usize::from(disable_from_next_line);
        for problem in problems {
            ranges
                .entry(problem)
                .or_default()
                .push((begin, last_line_no));
        }
    }
    for (line_no, problems) in directives(lines, Directive::Enable) {
        for problem in problems {
            for range in ranges.entry(problem).or_default() {
                range.1 = range.1.min(line_no);
            }
        }
    }
    ranges
}

#[derive(Clone, Copy)]
enum Directive {
    Ignore,
    Disable,
    Enable,
}

/// The lines holding `# gdlint: <directive>=a, b` and the stripped problem names each lists.
fn directives(lines: &[&str], directive: Directive) -> Vec<(usize, Vec<String>)> {
    static REGEXES: OnceLock<[Regex; 3]> = OnceLock::new();
    let regexes = REGEXES.get_or_init(|| {
        // Python's `\s` also matches U+001C..U+001F; `[^,]` cannot meet a newline within one line.
        ["ignore", "disable", "enable"].map(|word| {
            let s = r"[\s\x1C-\x1F]*";
            Regex::new(&format!("#{s}gdlint{s}:{s}{word}{s}={s}([^,]+(,[^,]+)*)"))
                .expect("directive pattern")
        })
    });
    let regex = &regexes[directive as usize];
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(captures) = regex.captures(line) {
            let names = captures[1]
                .split(',')
                .map(|p| text::python_strip(p).to_owned())
                .collect();
            found.push((index + 1, names));
        }
    }
    found
}
