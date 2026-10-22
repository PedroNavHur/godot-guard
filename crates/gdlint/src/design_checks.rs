//! `design_checks.py`.

use crate::ast::AbstractSyntaxTree;
use crate::config::Config;
use crate::helpers::is_function_public;
use crate::problem::Problem;
use crate::utils::{tree_column, tree_line};

pub fn lint(ast: &AbstractSyntaxTree, config: &Config) -> Vec<Problem> {
    let mut problems = Vec::new();
    if config.is_enabled("max-public-methods") {
        max_public_methods_check(config.int("max-public-methods"), ast, &mut problems);
    }
    if config.is_enabled("max-returns") {
        max_returns_check(config.int("max-returns"), ast, &mut problems);
    }
    if config.is_enabled("function-arguments-number") {
        function_args_num_check(config.int("function-arguments-number"), ast, &mut problems);
    }
    problems
}

fn exceeds(count: usize, threshold: i64) -> bool {
    count as i128 > threshold as i128
}

fn function_args_num_check(threshold: i64, ast: &AbstractSyntaxTree, problems: &mut Vec<Problem>) {
    for function in &ast.all_functions {
        if exceeds(function.parameters, threshold) {
            problems.push(Problem {
                name: "function-arguments-number",
                description: format!(
                    "Function \"{}\" has more than {threshold} arguments",
                    function.name
                ),
                line: tree_line(function.lark_node),
                column: tree_column(function.lark_node),
            });
        }
    }
}

fn max_public_methods_check(threshold: i64, ast: &AbstractSyntaxTree, problems: &mut Vec<Problem>) {
    for a_class in &ast.all_classes {
        let public_functions = a_class
            .function_names
            .iter()
            .filter(|name| is_function_public(name))
            .count();
        if exceeds(public_functions, threshold) {
            // Every class has a name, the global scope included, so the Python's "Global scope class"
            // branch never runs.
            problems.push(Problem {
                name: "max-public-methods",
                description: format!(
                    "\"Class {}\" has more than {threshold} public methods (functions)",
                    a_class.name
                ),
                line: tree_line(a_class.lark_node),
                column: tree_column(a_class.lark_node),
            });
        }
    }
}

fn max_returns_check(threshold: i64, ast: &AbstractSyntaxTree, problems: &mut Vec<Problem>) {
    for function in &ast.all_functions {
        let returns: Vec<_> = function
            .all_statements
            .iter()
            .filter(|s| s.data == "return_stmt")
            .collect();
        if exceeds(returns.len(), threshold) {
            // A negative threshold flags a function with no returns, where Python's `returns[-1]` raises.
            let last = returns
                .last()
                .expect("IndexError: max-returns below zero on a function without returns");
            problems.push(Problem {
                name: "max-returns",
                description: format!(
                    "Function \"{}\" has more than {threshold} return statements",
                    function.name
                ),
                line: tree_line(last),
                column: tree_column(last),
            });
        }
    }
}
