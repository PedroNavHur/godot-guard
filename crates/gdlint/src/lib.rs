//! gdlint: the same problems as gdtoolkit 4.5.0's linter.

pub mod config;
pub mod pyre;
pub mod repr;
pub mod utils;
pub mod yaml;

pub use config::{Config, ConfigError};
