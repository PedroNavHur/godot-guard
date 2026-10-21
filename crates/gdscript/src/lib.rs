//! GDScript lexer and parser: the tree gdtoolkit 4.5.0's lark grammar builds, without Python.

#[cfg(feature = "oracle")]
pub mod oracle;
pub mod tree;

pub use tree::{Node, Span, Token, Tree, intern};
