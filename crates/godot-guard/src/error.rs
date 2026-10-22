//! Why a command stopped, classified as 0.4.0's exit status tells them apart.

use std::fmt;

#[derive(Debug)]
pub enum Failure {
    /// A bad `.godot-guard.yml` (0.4.0's `ConfigError`): `godot-guard: MESSAGE` and exit 2.
    Config(String),
    /// A missing tool or a refused spawn (0.4.0's `ToolError`): exit 2, or status 127 inside a gate step.
    Tool(String),
    /// Anything 0.4.0 does not catch, such as a git or file error: it crashes with exit 1.
    Crash(String),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Failure::Config(message) | Failure::Tool(message) | Failure::Crash(message) => {
                f.write_str(message)
            }
        }
    }
}

impl Failure {
    /// A crash naming what failed and why.
    pub fn crash(context: impl fmt::Display, cause: impl fmt::Display) -> Failure {
        Failure::Crash(format!("{context}: {cause}"))
    }
}

pub type Result<T> = std::result::Result<T, Failure>;
