//! gdformat: byte-identical to gdtoolkit 4.5.0's formatter.
//!
//! One module per module of `gdtoolkit/formatter`, with the Python's names. The formatter reads the trees
//! `gdscript` parses; `node` adds the fake trees gdtoolkit builds while formatting, and `py` the Python
//! string semantics the output depends on.

// Some functions wait for their callers, which a later layer of the stack adds.
#![allow(dead_code)]

mod constants;
mod context;
mod expression_utils;
mod node;
mod py;
mod statement_utils;
mod types;
