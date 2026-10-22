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
        let Some(func_args) = func_args_of(func_def) else {
            continue;
        };
        problems.extend(unused_arguments(func_def, func_args));
    }
    problems
}

fn func_args_of(func_def: &Tree) -> Option<&Tree> {
    let func_header = child_tree(func_def, 0);
    match func_header.children.get(1) {
        Some(Node::Tree(func_args)) if func_args.data == "func_args" => Some(func_args),
        _ => None,
    }
}

/// The arguments a function defines: Python's dicts keep first-insertion order, `order` does that here.
struct ArgumentDefinitions<'a> {
    order: Vec<&'a str>,
    counts: HashMap<&'a str, usize>,
    tokens: HashMap<&'a str, &'a Token>,
}

fn argument_definitions(func_args: &Tree) -> ArgumentDefinitions<'_> {
    let mut definitions = ArgumentDefinitions {
        order: Vec::new(),
        counts: HashMap::new(),
        tokens: HashMap::new(),
    };
    let func_arg_trees = func_args
        .children
        .iter()
        .filter_map(Node::as_tree)
        .filter(|func_arg| func_arg.data != "trailing_comma");
    for func_arg in func_arg_trees {
        let holder = argument_holder(func_arg);
        let arg_name_token = find_name_token_among_children(holder)
            .unwrap_or_else(|| panic!("{} has no name token", holder.data));
        let arg_name = arg_name_token.value.as_str();
        let count = definitions.counts.entry(arg_name).or_insert(0);
        if *count == 0 {
            definitions.order.push(arg_name);
        }
        *count += 1;
        definitions.tokens.insert(arg_name, arg_name_token);
    }
    definitions
}

fn argument_holder(func_arg: &Tree) -> &Tree {
    match func_arg.data {
        "func_arg_variadic" => child_tree(func_arg, 0),
        _ => func_arg,
    }
}

fn name_occurrences(func_def: &Tree) -> HashMap<&str, usize> {
    let mut occurrences: HashMap<&str, usize> = HashMap::new();
    let name_tokens = func_def
        .iter_topdown()
        .flat_map(|xnode| xnode.children.iter().filter_map(Node::as_token))
        .filter(|node| node.kind == "NAME");
    for node in name_tokens {
        *occurrences.entry(node.value.as_str()).or_insert(0) += 1;
    }
    occurrences
}

fn unused_arguments(func_def: &Tree, func_args: &Tree) -> Vec<Problem> {
    let definitions = argument_definitions(func_args);
    let occurrences = name_occurrences(func_def);
    let mut problems = Vec::new();
    for argument in definitions.order {
        let occurred = *occurrences
            .get(argument)
            .unwrap_or_else(|| panic!("KeyError: argument {argument:?} is not a NAME token"));
        if definitions.counts[argument] == occurred && !argument.starts_with('_') {
            let token = definitions.tokens[argument];
            problems.push(Problem {
                name: "unused-argument",
                description: format!("unused function argument '{argument}'"),
                line: token.span.line,
                column: token.span.column,
            });
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
