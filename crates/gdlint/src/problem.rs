//! `problem.py` and `problem_printer.py`.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The check's name, such as `unused-argument`.
    pub name: &'static str,
    pub description: String,
    pub line: usize,
    pub column: usize,
}

/// `print_problem`'s line, without its newline: `path:line: Error: description (name)`.
pub struct Printed<'a> {
    pub problem: &'a Problem,
    pub file_path: &'a str,
}

impl fmt::Display for Printed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Printed { problem, file_path } = self;
        write!(
            f,
            "{file_path}:{}: Error: {} ({})",
            problem.line, problem.description, problem.name
        )
    }
}
