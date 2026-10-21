//! `if_return_checks.py`: no-elif-return and no-else-return.

use gdscript::{Node, Tree};

use crate::problem::Problem;
use crate::utils::{child_token, child_tree, find_stmts_among_children, tree_column, tree_line};

pub fn no_elif_return_check(subtrees: &[&Tree]) -> Vec<Problem> {
    let mut problems = Vec::new();
    for if_stmt in subtrees.iter().filter(|tree| tree.data == "if_stmt") {
        for elif_branch in find_elif_branches_to_remove(if_stmt) {
            problems.push(Problem {
                name: "no-elif-return",
                description: "Unnecessary \"elif\" after \"return\"".into(),
                line: tree_line(elif_branch),
                column: tree_column(elif_branch),
            });
        }
    }
    problems
}

pub fn no_else_return_check(subtrees: &[&Tree]) -> Vec<Problem> {
    let mut problems = Vec::new();
    for tree in subtrees.iter().filter(|tree| has_if_stmt(tree)) {
        let var_names = find_var_names(tree);
        for if_stmt in find_stmts_among_children(tree, "if_stmt") {
            problems.extend(check_else_problems(if_stmt, &var_names));
        }
    }
    problems
}

fn find_elif_branches_to_remove(if_stmt: &Tree) -> Vec<&Tree> {
    let non_else_branches = get_non_else_branches(if_stmt);
    let elif_branches: Vec<_> = get_if_stmt_branches(if_stmt)
        .filter(|b| b.data == "elif_branch")
        .collect();
    let mut elif_branches_to_remove = Vec::new();
    for (i, non_else_branch) in non_else_branches
        .iter()
        .enumerate()
        .take(non_else_branches.len().saturating_sub(1))
    {
        if !check_if_it_always_returns(non_else_branch) {
            break;
        }
        elif_branches_to_remove.push(elif_branches[i]);
    }
    elif_branches_to_remove
}

fn find_var_names(tree: &Tree) -> Vec<&str> {
    find_stmts_among_children(tree, "func_var_stmt")
        .into_iter()
        .map(|func_var_stmt| child_token(child_tree(func_var_stmt, 0), 0).value.as_str())
        .collect()
}

fn check_else_problems(if_stmt: &Tree, parent_var_names: &[&str]) -> Option<Problem> {
    let else_branch = find_else_branch_that_might_be_removed(if_stmt)?;
    let else_var_names = find_var_names(else_branch);
    if else_var_names
        .iter()
        .any(|name| parent_var_names.contains(name))
    {
        return None;
    }
    Some(Problem {
        name: "no-else-return",
        description: "Unnecessary \"else\" after \"return\"".into(),
        line: tree_line(else_branch),
        column: tree_column(else_branch),
    })
}

fn find_else_branch_that_might_be_removed(if_stmt: &Tree) -> Option<&Tree> {
    if !has_else_branch(if_stmt) {
        return None;
    }
    if !get_non_else_branches(if_stmt)
        .iter()
        .all(|branch| check_if_it_always_returns(branch))
    {
        return None;
    }
    get_if_stmt_branches(if_stmt).find(|branch| branch.data == "else_branch")
}

fn check_if_it_always_returns(tree: &Tree) -> bool {
    !find_stmts_among_children(tree, "return_stmt").is_empty()
        || find_stmts_among_children(tree, "if_stmt")
            .into_iter()
            .any(check_if_if_stmt_always_returns)
        || find_stmts_among_children(tree, "match_stmt")
            .into_iter()
            .any(check_if_match_stmt_always_returns)
}

fn check_if_if_stmt_always_returns(if_stmt: &Tree) -> bool {
    has_else_branch(if_stmt) && get_if_stmt_branches(if_stmt).all(check_if_it_always_returns)
}

fn check_if_match_stmt_always_returns(match_stmt: &Tree) -> bool {
    let has_wildcard = get_match_stmt_branches(match_stmt).any(is_wildcard_pattern_branch);
    has_wildcard && get_match_stmt_branches(match_stmt).all(check_if_it_always_returns)
}

fn has_if_stmt(tree: &Tree) -> bool {
    tree.subtrees().any(|child| child.data.ends_with("if_stmt"))
}

fn has_else_branch(if_stmt: &Tree) -> bool {
    get_if_stmt_branches(if_stmt).any(|branch| branch.data == "else_branch")
}

fn is_wildcard_pattern_branch(match_stmt_branch: &Tree) -> bool {
    let pattern = child_tree(match_stmt_branch, 0);
    matches!(pattern.children.first(), Some(Node::Tree(first)) if first.data == "wildcard_pattern")
}

/// An if statement's children, each a branch; the Python reads `.data` from each.
fn get_if_stmt_branches(if_stmt: &Tree) -> impl Iterator<Item = &Tree> {
    if_stmt.children.iter().map(as_branch)
}

fn get_match_stmt_branches(match_stmt: &Tree) -> impl Iterator<Item = &Tree> {
    match_stmt.children.iter().skip(1).map(as_branch)
}

fn get_non_else_branches(if_stmt: &Tree) -> Vec<&Tree> {
    get_if_stmt_branches(if_stmt)
        .filter(|branch| branch.data != "else_branch")
        .collect()
}

fn as_branch(node: &Node) -> &Tree {
    node.as_tree()
        .unwrap_or_else(|| panic!("branch {node:?} is a token"))
}
