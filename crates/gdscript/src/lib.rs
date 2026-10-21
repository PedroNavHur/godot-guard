//! GDScript lexer and parser: the tree gdtoolkit 4.5.0's lark grammar builds, without Python.

pub mod error;
mod generated;
mod indenter;
mod lalr;
mod lexer;
mod matchers;
#[cfg(feature = "oracle")]
pub mod oracle;
mod parser;
mod tables;
pub mod tree;
mod tree_builder;

pub use error::{ErrorKind, ParseError};
pub use parser::{parse, parse_comments};
pub use tree::{Node, Span, Token, Tree, intern};
