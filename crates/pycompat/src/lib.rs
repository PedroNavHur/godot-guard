//! The Python behavior the ports of gdtoolkit and godot-guard reproduce, in one place.
//!
//! Each module stands for the Python source it ports, under Python's names: `builtins`, `os`, `string`
//! (`str` methods), gdtoolkit's `common/utils.py`, and PyYAML's loader. Where the ports once kept their own
//! copies, Python's behavior decides what these do.

pub mod builtins;
pub mod os;
pub mod string;
pub mod utils;
