//! Port of gdtoolkit 4.5.0's `parser/parser.py`: the entry points its formatter and linter call.

use crate::error::ParseError;
use crate::generated;
use crate::lalr;
use crate::lexer::Language;
use crate::tree::Tree;

static COMMENTS: Language = Language::new(&generated::comments::GRAMMAR);

/// `parser.parse_comments(code)`: every comment, standalone and inline, as COMMENT tokens under `start`.
pub fn parse_comments(code: &str) -> Result<Tree, ParseError> {
    lalr::parse(&COMMENTS, code)
}
