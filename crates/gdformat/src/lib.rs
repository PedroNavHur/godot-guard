//! gdformat: byte-identical to gdtoolkit 4.5.0's formatter.
//!
//! One module per module of `gdtoolkit/formatter`, with the Python's names. The formatter reads the trees
//! `gdscript` parses; `node` adds the fake trees gdtoolkit builds while formatting, and `py` the Python
//! string semantics the output depends on.

// Some functions wait for their callers, which a later layer of the stack adds.
#![allow(dead_code)]

mod annotation;
mod const_statement;
mod constants;
mod context;
mod dot_chain;
mod expression;
mod expression_to_str;
mod expression_utils;
mod function_statement_to_str;
mod node;
mod py;
mod statement_utils;
mod types;
mod var_statement;
