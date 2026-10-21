//! gdformat's two parses (`parser.parse`, then `parser.parse_comments`) and the text it prints when
//! one fails.

use std::fmt;

use gdscript::{ErrorKind, ParseError};

use crate::safety_checks::ParsedCode;

/// A parse error, shown as gdformat shows it: lark's context line and caret first for an unexpected
/// token (`lark_unexpected_token_to_str`), the error alone otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseFailure {
    pub error: ParseError,
    context: Option<String>,
}

impl fmt::Display for ParseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{context}\n{}", self.error),
            None => write!(f, "{}", self.error),
        }
    }
}

impl std::error::Error for ParseFailure {}

/// The code tree and the comment tree of `code`, which must be read as Python reads text files.
pub fn parse_code(code: &str) -> Result<ParsedCode, ParseFailure> {
    let failure = |error: ParseError| ParseFailure {
        context: unexpected_token_context(&error, code),
        error,
    };
    let tree = gdscript::parse(code).map_err(failure)?;
    let comments = gdscript::parse_comments(code).map_err(failure)?;
    Ok(ParsedCode { tree, comments })
}

/// lark's `UnexpectedInput.get_context(code)`, span 40: the line around the error and a caret under
/// it. None where lark's raises and gdtoolkit falls back to the bare error: a token without a position.
fn unexpected_token_context(error: &ParseError, code: &str) -> Option<String> {
    let ErrorKind::UnexpectedToken { .. } = error.kind else {
        return None;
    };
    let chars: Vec<char> = code.chars().collect();
    let pos = position(&chars, error.line?, error.column?);
    let start = pos.saturating_sub(40).min(chars.len());
    let end = (pos + 40).min(chars.len());
    let pos = pos.min(chars.len());
    let before: String = chars[start..pos].iter().collect();
    let before = before.rsplit('\n').next().unwrap_or_default();
    let after: String = chars[pos..end].iter().collect();
    let after = after.split('\n').next().unwrap_or_default();
    let caret = " ".repeat(expanded_len(before));
    Some(format!("{before}{after}\n{caret}^\n"))
}

/// The code-point offset of a 1-based line and column, as lark's `pos_in_stream`.
fn position(chars: &[char], line: usize, column: usize) -> usize {
    let line_start = match line {
        0 | 1 => 0,
        _ => chars
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == '\n')
            .nth(line - 2)
            .map_or(chars.len(), |(at, _)| at + 1),
    };
    line_start + column.saturating_sub(1)
}

/// `len(s.expandtabs())`: tabs advance to the next multiple of 8.
fn expanded_len(s: &str) -> usize {
    s.chars().fold(0, |column, c| match c {
        '\t' => column + 8 - column % 8,
        '\n' | '\r' => 0,
        _ => column + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_at_the_unexpected_token() {
        let failure = parse_code("var x = 1\nvar = 2\n").unwrap_err();
        let shown = failure.to_string();
        assert!(
            shown.starts_with("var = 2\n    ^\n\nUnexpected token"),
            "{shown}"
        );
    }

    #[test]
    fn expands_tabs_like_python() {
        assert_eq!(expanded_len("\tab"), 10);
        assert_eq!(expanded_len("abc\td"), 9);
    }

    #[test]
    fn parses_both_trees() {
        let parsed = parse_code("# note\nvar x = 1\n").unwrap();
        assert_eq!(parsed.tree.data, "start");
        assert_eq!(parsed.comments.data, "start");
    }
}
