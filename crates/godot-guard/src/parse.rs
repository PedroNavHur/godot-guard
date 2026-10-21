//! Parse GDScript sources into gdtoolkit's tree, the one seam between the rules and the parser.

use gdscript::{ErrorKind, Tree};
use rayon::prelude::*;

/// A parse tree, or the line and first message line of the error gdtoolkit raises.
#[derive(Debug)]
pub enum Parsed {
    Tree(Tree),
    Error { line: usize, message: String },
}

/// Each source's tree or parse error, in order, parsed in parallel.
pub fn parse_all(sources: &[&str]) -> Vec<Parsed> {
    sources.par_iter().map(|source| parse(source)).collect()
}

/// `rules.tree_rules`'s reading of a parse error: line 1 where lark has none, and the first line of
/// the exception's text, or its type's name when the text is empty.
pub fn parse(source: &str) -> Parsed {
    match gdscript::parse(source) {
        Ok(tree) => Parsed::Tree(tree),
        Err(error) => {
            let message = match &error.kind {
                ErrorKind::Indenter(what) => match what.split_once(": ") {
                    Some(("AssertionError", _)) | None => {
                        what.split(": ").next().unwrap_or(what).to_owned()
                    }
                    Some((_, text)) => text.to_owned(),
                },
                _ => error
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            };
            Parsed::Error {
                line: error.line.unwrap_or(1).max(1),
                message,
            }
        }
    }
}
