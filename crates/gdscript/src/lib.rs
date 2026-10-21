//! GDScript lexer and parser: the tree gdtoolkit 4.5.0's lark grammar builds, without Python.

#[allow(dead_code, reason = "read by the parser runtime added next")]
mod generated;
#[cfg(test)]
mod matchers;
#[cfg(feature = "oracle")]
pub mod oracle;
#[allow(dead_code, reason = "read by the parser runtime added next")]
mod tables;
pub mod tree;

pub use tree::{Node, Span, Token, Tree, intern};
