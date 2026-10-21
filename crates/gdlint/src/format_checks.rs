//! `format_checks.py`: checks that read the source text, line by line.

use crate::config::Config;
use crate::problem::Problem;
use crate::text::{is_python_space, splitlines};

pub fn lint(gdscript_code: &str, config: &Config) -> Vec<Problem> {
    let lines = splitlines(gdscript_code);
    let mut problems = Vec::new();
    if config.is_enabled("max-line-length") {
        max_line_length_check(
            config.int("max-line-length"),
            config.int("tab-characters"),
            &lines,
            &mut problems,
        );
    }
    if config.is_enabled("max-file-lines") {
        max_file_lines_check(config.int("max-file-lines"), &lines, &mut problems);
    }
    if config.is_enabled("trailing-whitespace") {
        line_check(
            &lines,
            "trailing-whitespace",
            "Trailing whitespace(s)",
            has_trailing_whitespace,
            &mut problems,
        );
    }
    if config.is_enabled("mixed-tabs-and-spaces") {
        line_check(
            &lines,
            "mixed-tabs-and-spaces",
            "Mixed tabs and spaces",
            has_mixed_indent,
            &mut problems,
        );
    }
    problems
}

fn line_problem(name: &'static str, description: String, line: usize) -> Problem {
    Problem {
        name,
        description,
        line,
        column: 0,
    }
}

fn max_line_length_check(
    threshold: i64,
    tab_characters: i64,
    lines: &[&str],
    problems: &mut Vec<Problem>,
) {
    // `line.replace("\t", " " * n)` counts each tab as n code points; `" " * n` is empty for n < 0.
    let tab_width = tab_characters.max(0) as usize;
    for (line_number, line) in lines.iter().enumerate() {
        let tabs = line.chars().filter(|&c| c == '\t').count();
        let length = line.chars().count() - tabs + tabs * tab_width;
        if length as i128 > threshold as i128 {
            let description = format!("Max allowed line length ({threshold}) exceeded");
            problems.push(line_problem(
                "max-line-length",
                description,
                line_number + 1,
            ));
        }
    }
}

fn max_file_lines_check(threshold: i64, lines: &[&str], problems: &mut Vec<Problem>) {
    if lines.len() as i128 > threshold as i128 {
        let description = format!("Max allowed file lines num ({threshold}) exceeded");
        problems.push(line_problem("max-file-lines", description, lines.len()));
    }
}

fn line_check(
    lines: &[&str],
    name: &'static str,
    description: &str,
    is_problem: fn(&str) -> bool,
    problems: &mut Vec<Problem>,
) {
    for (line_number, line) in lines.iter().enumerate() {
        if is_problem(line) {
            problems.push(line_problem(name, description.to_owned(), line_number + 1));
        }
    }
}

/// `re.search(r"\s$", line)`; a line from `splitlines` holds no newline for `$` to stop before.
fn has_trailing_whitespace(line: &str) -> bool {
    line.chars().next_back().is_some_and(is_python_space)
}

/// `re.search("^(\t+ +| +\t+)", line)`.
fn has_mixed_indent(line: &str) -> bool {
    let mut chars = line.chars();
    match chars.next() {
        Some(first @ ('\t' | ' ')) => {
            let other = if first == '\t' { ' ' } else { '\t' };
            chars.find(|&c| c != first) == Some(other)
        }
        _ => false,
    }
}
