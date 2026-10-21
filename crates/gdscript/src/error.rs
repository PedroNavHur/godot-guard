//! The errors gdtoolkit's parse raises, kept apart the way its callers tell them apart: gdformat and
//! gdlint print `UnexpectedToken` with context, other `UnexpectedInput` plainly, and `DedentError` alone.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based, as lark reports it; None where lark has none (`DedentError`, or a token the indenter
    /// made at the end of input).
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub kind: ErrorKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// lark's `UnexpectedToken`: the parser had no action for `token`. The contextual lexer raises it too,
    /// when only a terminal outside the current state matches.
    UnexpectedToken {
        token: &'static str,
        value: String,
        expected: Vec<&'static str>,
    },
    /// lark's `UnexpectedCharacters`: no terminal matches at all.
    UnexpectedCharacters { character: char },
    /// lark's `DedentError`.
    Dedent { indent: usize, expected: usize },
    /// A Python exception other than lark's that gdtoolkit's indenter can raise on bad input (an
    /// `AssertionError` on unbalanced brackets).
    Indenter(&'static str),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = |f: &mut fmt::Formatter<'_>| match (self.line, self.column) {
            (Some(line), Some(column)) => write!(f, " at line {line}, column {column}"),
            (Some(line), None) => write!(f, " at line {line}"),
            _ => Ok(()),
        };
        match &self.kind {
            ErrorKind::UnexpectedToken {
                token,
                value,
                expected,
            } => {
                write!(f, "Unexpected token Token('{token}', {value:?})")?;
                at(f)?;
                write!(f, ".\nExpected one of: ")?;
                for name in expected {
                    write!(f, "\n\t* {name}")?;
                }
                Ok(())
            }
            ErrorKind::UnexpectedCharacters { character } => {
                write!(
                    f,
                    "No terminal matches '{character}' in the current parser context"
                )?;
                at(f)
            }
            ErrorKind::Dedent { indent, expected } => write!(
                f,
                "Unexpected dedent to column {indent}. Expected dedent to {expected}"
            ),
            ErrorKind::Indenter(what) => f.write_str(what),
        }
    }
}

impl std::error::Error for ParseError {}
