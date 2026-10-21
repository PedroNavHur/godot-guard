//! Port of gdtoolkit 4.5.0's `parser/parser.py`: the two entry points its formatter and linter call.

use crate::error::ParseError;
use crate::generated;
use crate::lalr;
use crate::lexer::Language;
use crate::tree::Tree;

static GDSCRIPT: Language = Language::new(&generated::gdscript::GRAMMAR);
static COMMENTS: Language = Language::new(&generated::comments::GRAMMAR);

/// `parser.parse(code, gather_metadata=True)`: the tree gdtoolkit's formatter and linter work from.
pub fn parse(code: &str) -> Result<Tree, ParseError> {
    // Adding a newline at the end of the code works around a few corner cases not addressed in the grammar.
    let mut adjusted_code = String::with_capacity(code.len() + 1);
    adjusted_code.push_str(code);
    adjusted_code.push('\n');
    lalr::parse(&GDSCRIPT, &adjusted_code)
}

/// `parser.parse_comments(code)`: every comment, standalone and inline, as COMMENT tokens under `start`.
pub fn parse_comments(code: &str) -> Result<Tree, ParseError> {
    lalr::parse(&COMMENTS, code)
}
