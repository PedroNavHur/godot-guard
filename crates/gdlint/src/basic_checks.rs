//! `basic_checks.py`.

use std::collections::{HashMap, HashSet};

use gdscript::{Node, Token, Tree};

use crate::config::Config;
use crate::problem::Problem;
use crate::utils::{
    child_token, child_tree, find_data, find_name_token_among_children, find_stmts_among_children,
    get_column, get_line, nodes_equal, tree_column, tree_line,
};

type TreeCheck = fn(&[&Tree]) -> Vec<Problem>;

pub fn lint(subtrees: &[&Tree], config: &Config) -> Vec<Problem> {
    let checks: [(&str, TreeCheck); 5] = [
        ("unnecessary-pass", unnecessary_pass_check),
        ("expression-not-assigned", expression_not_assigned_check),
        ("duplicated-load", duplicated_load_check),
        ("unused-argument", unused_argument_check),
        ("comparison-with-itself", comparison_with_itself_check),
    ];
    checks
        .into_iter()
        .filter(|(name, _)| config.is_enabled(name))
        .flat_map(|(_, check)| check(subtrees))
        .collect()
}

fn unnecessary_pass_check(subtrees: &[&Tree]) -> Vec<Problem> {
    let mut problems = Vec::new();
    for node in subtrees {
        let pass_stmts = find_stmts_among_children(node, "pass_stmt");
        let all_stmts = find_stmts_among_children(node, "_stmt");
        if pass_stmts.len() < all_stmts.len() {
            for pass_stmt in pass_stmts {
                problems.push(Problem {
                    name: "unnecessary-pass",
                    description: "\"pass\" statement not necessary".into(),
                    line: tree_line(pass_stmt),
                    column: tree_column(pass_stmt),
                });
            }
        }
    }
    problems
}

/// `remove_outer_parentheses` from `gdtoolkit/formatter/expression_utils.py`.
fn remove_outer_parentheses(expression: &Node) -> &Node {
    match expression {
        Node::Tree(tree) if tree.data == "par_expr" => remove_outer_parentheses(&tree.children[0]),
        _ => expression,
    }
}

const ASSIGNED_EXPRESSIONS: [&str; 6] = [
    "assnmnt_expr",
    "await_expr",
    "standalone_call",
    "getattr_call",
    "string",
    "lambda",
];

fn expression_not_assigned_check(subtrees: &[&Tree]) -> Vec<Problem> {
    let mut problems = Vec::new();
    for expr_stmt in find_data(subtrees, "expr_stmt") {
        let expr = child_tree(expr_stmt, 0);
        let actual_expression = remove_outer_parentheses(&expr.children[0]);
        let is_assigned = matches!(actual_expression, Node::Tree(tree) if ASSIGNED_EXPRESSIONS.contains(&tree.data));
        if !is_assigned {
            problems.push(Problem {
                name: "expression-not-assigned",
                description: "expression is not asigned, and hence it can be removed".into(),
                line: get_line(actual_expression),
                column: get_column(actual_expression),
            });
        }
    }
    problems
}

fn duplicated_load_check(subtrees: &[&Tree]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let mut loaded_strings: HashSet<&str> = HashSet::new();
    let mut calls = find_data(subtrees, "standalone_call");
    // `sorted` is stable, like `sort_by_key`.
    calls.sort_by_key(|call| tree_line(call));
    for call in calls {
        let callee_name = child_token(call, 0).value.as_str();
        let Some(Node::Tree(string_rule)) = call.children.get(1) else {
            continue;
        };
        if !matches!(callee_name, "load" | "preload") || string_rule.data != "string" {
            continue;
        }
        let loaded_string = child_token(string_rule, 0).value.as_str();
        if !loaded_strings.insert(loaded_string) {
            problems.push(Problem {
                name: "duplicated-load",
                description: format!("duplicated loading of {loaded_string}"),
                line: tree_line(string_rule),
                column: tree_column(string_rule),
            });
        }
    }
    problems
}

fn unused_argument_check(subtrees: &[&Tree]) -> Vec<Problem> {
    let mut problems = Vec::new();
    for func_def in find_data(subtrees, "func_def") {
        let func_header = child_tree(func_def, 0);
        let Some(Node::Tree(func_args)) = func_header.children.get(1) else {
            continue;
        };
        if func_args.data != "func_args" {
            continue;
        }
        // Python's dicts keep first-insertion order; `order` does that here.
        let mut order: Vec<&str> = Vec::new();
        let mut argument_definitions: HashMap<&str, usize> = HashMap::new();
        let mut argument_tokens: HashMap<&str, &Token> = HashMap::new();
        for func_arg in func_args.children.iter().filter_map(Node::as_tree) {
            if func_arg.data == "trailing_comma" {
                continue;
            }
            let holder = if func_arg.data == "func_arg_variadic" {
                child_tree(func_arg, 0)
            } else {
                func_arg
            };
            let arg_name_token = find_name_token_among_children(holder)
                .unwrap_or_else(|| panic!("{} has no name token", holder.data));
            let arg_name = arg_name_token.value.as_str();
            let count = argument_definitions.entry(arg_name).or_insert(0);
            if *count == 0 {
                order.push(arg_name);
            }
            *count += 1;
            argument_tokens.insert(arg_name, arg_name_token);
        }
        let mut name_occurances: HashMap<&str, usize> = HashMap::new();
        for xnode in func_def.iter_topdown() {
            for node in xnode.children.iter().filter_map(Node::as_token) {
                if node.kind == "NAME" {
                    *name_occurances.entry(node.value.as_str()).or_insert(0) += 1;
                }
            }
        }
        for argument in order {
            let occurrences = *name_occurances
                .get(argument)
                .unwrap_or_else(|| panic!("KeyError: argument {argument:?} is not a NAME token"));
            if argument_definitions[argument] == occurrences && !argument.starts_with('_') {
                let token = argument_tokens[argument];
                problems.push(Problem {
                    name: "unused-argument",
                    description: format!("unused function argument '{argument}'"),
                    line: token.span.line,
                    column: token.span.column,
                });
            }
        }
    }
    problems
}

fn comparison_with_itself_check(subtrees: &[&Tree]) -> Vec<Problem> {
    let mut problems = Vec::new();
    for comparison in find_data(subtrees, "comparison") {
        assert_eq!(
            comparison.children.len(),
            3,
            "a comparison has three children"
        );
        if nodes_equal(&comparison.children[0], &comparison.children[2]) {
            problems.push(Problem {
                name: "comparison-with-itself",
                description: "Redundant comparison".into(),
                line: tree_line(comparison),
                column: tree_column(comparison),
            });
        }
    }
    problems
}
