//! `class_checks.py`.

use crate::ast::{AbstractSyntaxTree, Class, Statement, annotation_name};
use crate::config::Config;
use crate::helpers::is_function_public;
use crate::problem::Problem;
use crate::utils::{child_tree, find_name_token_among_children, tree_column, tree_line};

pub fn lint(ast: &AbstractSyntaxTree, config: &Config) -> Vec<Problem> {
    if !config.is_enabled("class-definitions-order") {
        return Vec::new();
    }
    let order = config.class_definitions_order();
    ast.all_classes
        .iter()
        .flat_map(|a_class| class_definitions_order_check_for_class(a_class, order))
        .collect()
}

fn class_definitions_order_check_for_class(a_class: &Class, order: &[String]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut current_section = order[0].clone();
    for statement in a_class
        .statements
        .iter()
        .filter(|s| !is_statement_irrelevant(s))
    {
        let line = tree_line(statement.lark_node);
        let column = tree_column(statement.lark_node);
        let statement_section = map_statement_to_section(statement);
        let rank = |section: &str| order.iter().position(|s| s == section);
        // `current_section` always comes from `order`, so only the statement's section can be missing.
        let current_section_rank =
            rank(&current_section).expect("the current section is in the order");
        match rank(&statement_section) {
            Some(section_rank) if section_rank >= current_section_rank => current_section = statement_section,
            Some(_) => problems.push(Problem {
                name: "class-definitions-order",
                description: format!("Definition out of order in {}", a_class.name),
                line,
                column,
            }),
            None => problems.push(Problem {
                name: "class-definitions-order",
                description: format!(
                    "Definition order not specified for '{current_section}' or '{statement_section}', \
                     please fix/re-generate your gdlintrc file"
                ),
                line,
                column,
            }),
        }
    }
    problems
}

fn is_statement_irrelevant(statement: &Statement) -> bool {
    match statement.kind {
        "pass_stmt" | "class_def" => true,
        "annotation" => annotation_name(statement.lark_node) != "tool",
        _ => false,
    }
}

fn map_statement_to_section(statement: &Statement) -> String {
    let section = match statement.kind {
        "class_var_stmt" => {
            if statement
                .annotations
                .iter()
                .any(|name| name.starts_with("export"))
            {
                return "exports".into();
            }
            let visibility = class_var_stmt_visibility(statement);
            if statement.annotations.contains(&"onready") {
                return format!("onready{visibility}vars");
            }
            return format!("{visibility}vars");
        }
        "signal_stmt" => "signals",
        "extends_stmt" | "classname_extends_stmt" => "extends",
        "enum_stmt" => "enums",
        "classname_stmt" => "classnames",
        "const_stmt" => "consts",
        // Irrelevant annotations are filtered out first, so an annotation here is `@tool`.
        "annotation" => "tools",
        "func_def" | "static_func_def" | "abstract_func_def" => "others",
        "docstr_stmt" => "docstrings",
        "static_class_var_stmt" => "staticvars",
        other => panic!("NotImplementedError: no class section for {other}"),
    };
    section.into()
}

fn class_var_stmt_visibility(statement: &Statement) -> &'static str {
    let some_var_stmt = child_tree(statement.lark_node, 0);
    let name_token = find_name_token_among_children(some_var_stmt).expect("a variable has a name");
    if is_function_public(&name_token.value) {
        "pub"
    } else {
        "prv"
    }
}
