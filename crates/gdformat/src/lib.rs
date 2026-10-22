//! gdformat: byte-identical to gdtoolkit 4.5.0's formatter.
//!
//! One module per module of `gdtoolkit/formatter`, with the Python's names. The formatter reads the trees
//! `gdscript` parses; `node` adds the fake trees gdtoolkit builds while formatting, and `py` the Python
//! string semantics the output depends on.

mod annotation;
mod args;
mod block;
mod class_statement;
pub mod cli;
mod comments;
pub mod config;
mod const_statement;
mod constants;
mod context;
mod dot_chain;
mod expression;
mod expression_to_str;
mod expression_utils;
mod files;
mod formatter;
mod function_statement;
mod function_statement_to_str;
mod node;
mod parse;
mod property;
mod py;
mod safety_checks;
mod statement_utils;
mod types;
mod var_statement;

pub use files::{ReadError, read_text};
pub use formatter::format_code;
pub use parse::{ParseFailure, parse_code};
pub use safety_checks::{ParsedCode, SafetyError, SafetyViolation, check_formatting_safety};
