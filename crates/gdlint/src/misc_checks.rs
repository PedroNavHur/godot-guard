//! `misc_checks.py`.

use gdscript::Tree;

use crate::config::Config;
use crate::if_return_checks::{no_elif_return_check, no_else_return_check};
use crate::problem::Problem;

pub fn lint(subtrees: &[&Tree], config: &Config) -> Vec<Problem> {
    let mut problems = Vec::new();
    if config.is_enabled("no-elif-return") {
        problems.extend(no_elif_return_check(subtrees));
    }
    if config.is_enabled("no-else-return") {
        problems.extend(no_else_return_check(subtrees));
    }
    problems
}
