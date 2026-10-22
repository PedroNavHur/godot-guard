//! Comment runs and comment stripping, ported from pedro-best-practices `long_comments.py` (GDScript only).
//!
//! A comment is an unbroken run of `#` lines that each own their line. Prose lines exclude blank lines,
//! separator rules, references (a lone URL or path, a ticket such as ENG-123), `Run:` lines, and tooling
//! directives. `##` doc comments, trailing comments, license text, and the file header block above the
//! first line of code are exempt.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use pycompat::string;

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a valid built-in pattern")
}

static DIRECTIVE: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r"(?i)^(!|-\*-|(es|ox|ts|style)lint|biome-|prettier-|istanbul |c8 |svelte-ignore|@ts-|global |",
        r"noqa|type:|pyright:|mypy:|pylint:|fmt:|isort:|ruff:|pragma|phpcs|@?phpstan|@?psalm|@codeCoverage|",
        r"gdlint|warning_ignore|#?(end)?region\b|run:)"
    ))
});
static REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"(?i)^(see:?\s+)?(https?://\S+|[\w.-]*/[\w./#-]+|[A-Z][A-Z0-9]+-\d+)[.)]?$")
});
static SEPARATOR: LazyLock<Regex> = LazyLock::new(|| compile(r"^[-=*#/_~+]{3,}$"));
static LICENSE: LazyLock<Regex> = LazyLock::new(|| {
    compile(r"(?i)SPDX-License-Identifier|Copyright \(c\)|Copyright \d{4}|Licensed under")
});
static PREAMBLE: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^(extends\b|class_name\b|@tool\b|@icon\b|##|$)"));
// Strings first, so a `#` inside one (including &"" and ^"" literals) is not a comment.
static CODE_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    compile(concat!(
        r#"(?ms)"""(?:\\.|[^\\])*?(?:"""|\z)|'''(?:\\.|[^\\])*?(?:'''|\z)"#,
        r#"|"(?:\\.|[^"\\\n])*(?:"|$)|'(?:\\.|[^'\\\n])*(?:'|$)|#[^\n]*"#
    ))
});

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub line: usize,
    pub end_line: usize,
    pub body: Vec<String>,
}

impl Comment {
    pub fn prose(&self) -> usize {
        self.body.iter().filter(|text| is_prose(text)).count()
    }
}

fn is_prose(text: &str) -> bool {
    let text = string::strip(string::strip(text).trim_start_matches('*'));
    !text.is_empty()
        && !(DIRECTIVE.is_match(text) || REFERENCE.is_match(text) || SEPARATOR.is_match(text))
}

/// A line comment's text, or None when the line is not one.
fn comment_text(stripped: &str) -> Option<&str> {
    if stripped.starts_with("##") {
        None
    } else {
        stripped.strip_prefix('#')
    }
}

fn comment_runs(lines: &[&str]) -> Vec<Comment> {
    let mut runs: Vec<Comment> = Vec::new();
    let mut in_run = false;
    for (index, raw) in lines.iter().enumerate() {
        let number = index + 1;
        let Some(text) = comment_text(string::strip(raw)) else {
            in_run = false;
            continue;
        };
        if !in_run {
            runs.push(Comment {
                line: number,
                end_line: number,
                body: Vec::new(),
            });
            in_run = true;
        }
        let run = runs.last_mut().expect("a run was just started");
        run.end_line = number;
        run.body.push(text.to_owned());
    }
    runs
}

fn first_code_line(lines: &[&str], comments: &[Comment]) -> usize {
    let commented: HashSet<usize> = comments.iter().flat_map(|c| c.line..=c.end_line).collect();
    lines
        .iter()
        .enumerate()
        .map(|(index, raw)| (index + 1, raw))
        .find(|(number, raw)| !commented.contains(number) && !PREAMBLE.is_match(string::strip(raw)))
        .map_or(lines.len() + 1, |(number, _)| number)
}

/// Comments over `cap` prose lines, outside the file header block.
pub fn long_comments(source: &str, cap: i64) -> Vec<Comment> {
    let lines = string::splitlines(source);
    let comments = comment_runs(&lines);
    let first_code = first_code_line(&lines, &comments);
    comments
        .into_iter()
        .filter(|c| {
            i64::try_from(c.prose()).is_ok_and(|prose| prose > cap)
                && c.end_line >= first_code
                && !LICENSE.is_match(&c.body.join("\n"))
        })
        .collect()
}

/// The source with every comment removed and strings and line breaks kept.
pub fn strip_comments(source: &str) -> String {
    CODE_TOKEN
        .replace_all(source, |found: &regex::Captures| {
            let token = &found[0];
            if token.starts_with('#') {
                String::new()
            } else {
                token.to_owned()
            }
        })
        .into_owned()
}
