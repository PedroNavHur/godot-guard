//! What `_lint_file` prints under `path:` for a file gdtoolkit cannot parse, built from a `gdscript::ParseError`.
//!
//! An `UnexpectedToken` prints lark's `get_context` above the exception; one lark gave no position prints the
//! exception alone, stripped, as `lark_unexpected_token_to_str` falls back to. `UnexpectedCharacters` prints
//! its message and context, stripped; a `DedentError`, its message.
// ceiling: expected terminals are the parser's, in its order (lark's `accepts` is hidden, its order varies);
// "Previous tokens" is left out, as ParseError holds no earlier token. Parity compares neither.

use gdscript::{ErrorKind, ParseError};

use pycompat::builtins::repr;
use pycompat::string::strip;

/// The report for `error`, raised parsing `code`, or Err with the line Python's traceback would end on, for
/// the indenter's errors gdlint does not catch.
pub fn report(error: &ParseError, code: &str) -> Result<String, String> {
    let position = error.line.zip(error.column);
    match &error.kind {
        ErrorKind::UnexpectedToken {
            token,
            value,
            expected,
        } => {
            let (line, column) = match position {
                Some((line, column)) => (line.to_string(), column.to_string()),
                None => ("None".into(), "None".into()),
            };
            let message = format!(
                "Unexpected token Token({}, {}) at line {line}, column {column}.\nExpected one of: \n\t* {}\n",
                repr(token),
                repr(value),
                expected.join("\n\t* ")
            );
            Ok(match position {
                Some((line, column)) => format!("{}\n{message}", context(code, line, column)),
                None => strip(&message).to_owned(),
            })
        }
        ErrorKind::UnexpectedCharacters { character } => {
            let (line, column) = position.expect("lark positions UnexpectedCharacters");
            let message = format!(
                "No terminal matches '{character}' in the current parser context, at line {line} col {column}\n\n{}",
                context(code, line, column)
            );
            Ok(strip(&message).to_owned())
        }
        ErrorKind::Dedent { .. } => Ok(error.to_string()),
        ErrorKind::Indenter(what) => Err((*what).to_owned()),
    }
}

/// lark's `get_context(text, span=40)` at the 1-based `line` and `column`, which count characters, as
/// lark's line counter does: the line around the position, at most 40 characters each side, and a caret.
fn context(code: &str, line: usize, column: usize) -> String {
    let chars: Vec<char> = code.chars().collect();
    let line_start = if line <= 1 {
        0
    } else {
        let mut newlines = chars.iter().enumerate().filter(|(_, c)| **c == '\n');
        newlines
            .nth(line - 2)
            .map_or(chars.len(), |(index, _)| index + 1)
    };
    let pos = (line_start + column - 1).min(chars.len());
    let window_start = pos.saturating_sub(40);
    let before = &chars[window_start..pos];
    let before = match before.iter().rposition(|c| *c == '\n') {
        Some(index) => &before[index + 1..],
        None => before,
    };
    let after = &chars[pos..(pos + 40).min(chars.len())];
    let after = match after.iter().position(|c| *c == '\n') {
        Some(index) => &after[..index],
        None => after,
    };
    let mut out: String = before.iter().chain(after).collect();
    out.push('\n');
    out.push_str(&" ".repeat(expanded_width(before)));
    out.push_str("^\n");
    out
}

/// `len(text.expandtabs())`: tabs advance to the next multiple of 8; a carriage return restarts the count.
fn expanded_width(text: &[char]) -> usize {
    let (mut total, mut column) = (0, 0);
    for c in text {
        match c {
            '\t' => {
                let pad = 8 - column % 8;
                total += pad;
                column += pad;
            }
            '\r' | '\n' => {
                total += 1;
                column = 0;
            }
            _ => {
                total += 1;
                column += 1;
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_points_under_the_error() {
        assert_eq!(
            context("func f():\n\tvar x = )\n", 2, 10),
            "\tvar x = )\n                ^\n"
        );
        assert_eq!(context("x", 1, 1), "x\n^\n");
        let long = format!("var s = \"{}\" )", "a".repeat(50));
        assert_eq!(
            context(&long, 1, 62),
            format!("{}\" )\n{}^\n", "a".repeat(38), " ".repeat(40))
        );
    }

    #[test]
    fn expandtabs_counts_tab_stops() {
        let text: Vec<char> = "a\tbc\t".chars().collect();
        assert_eq!(expanded_width(&text), 16);
    }
}
