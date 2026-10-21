//! `gdtoolkit/common/utils.py`, plus the lark `Tree` behaviour the checks lean on: `iter_subtrees` order and
//! `==`, which compares rule names, token types and text but not positions.

use gdscript::{Node, Token, Tree};

/// lark's `Tree.iter_subtrees`: every subtree, children before parents, siblings left to right.
///
/// lark builds a queue in which each tree's children follow in reverse, then reverses the whole queue;
/// this does the same, so problems come out in the order the Python finds them.
pub fn iter_subtrees(tree: &Tree) -> Vec<&Tree> {
    let mut queue = vec![tree];
    let mut index = 0;
    while index < queue.len() {
        let subtree = queue[index];
        queue.extend(subtree.children.iter().rev().filter_map(Node::as_tree));
        index += 1;
    }
    queue.reverse();
    queue
}

/// lark's `find_data`, over subtrees already listed by `iter_subtrees`.
pub fn find_data<'a>(subtrees: &[&'a Tree], data: &str) -> Vec<&'a Tree> {
    subtrees
        .iter()
        .copied()
        .filter(|tree| tree.data == data)
        .collect()
}

/// lark's `==` between two nodes.
pub fn nodes_equal(a: &Node, b: &Node) -> bool {
    match (a, b) {
        (Node::Tree(a), Node::Tree(b)) => trees_equal(a, b),
        (Node::Token(a), Node::Token(b)) => a.kind == b.kind && a.value == b.value,
        _ => false,
    }
}

fn trees_equal(a: &Tree, b: &Tree) -> bool {
    a.data == b.data
        && a.children.len() == b.children.len()
        && a.children
            .iter()
            .zip(&b.children)
            .all(|(x, y)| nodes_equal(x, y))
}

pub fn find_name_token_among_children(tree: &Tree) -> Option<&Token> {
    tree.children
        .iter()
        .filter_map(Node::as_token)
        .find(|token| matches!(token.kind, "NAME" | "GET" | "SET"))
}

pub fn find_tree_among_children<'a>(tree_name_to_find: &str, tree: &'a Tree) -> Option<&'a Tree> {
    tree.subtrees()
        .find(|child| child.data == tree_name_to_find)
}

/// The tree's position. A tree with no tokens has none; Python raises AttributeError reading it.
fn meta(tree: &Tree) -> gdscript::Span {
    tree.meta
        .unwrap_or_else(|| panic!("tree {} holds no token, so it has no position", tree.data))
}

pub fn get_line(node: &Node) -> usize {
    match node {
        Node::Tree(tree) => meta(tree).line,
        Node::Token(token) => token.span.line,
    }
}

pub fn get_column(node: &Node) -> usize {
    match node {
        Node::Tree(tree) => meta(tree).column,
        Node::Token(token) => token.span.column,
    }
}

pub fn tree_line(tree: &Tree) -> usize {
    meta(tree).line
}

pub fn tree_column(tree: &Tree) -> usize {
    meta(tree).column
}

/// The child at `index` as a tree; the Python reads `.children` or `.data` there and crashes on a token.
pub fn child_tree(tree: &Tree, index: usize) -> &Tree {
    match tree.children.get(index) {
        Some(Node::Tree(child)) => child,
        other => panic!("{} child {index} is {other:?}, not a tree", tree.data),
    }
}

/// The child at `index` as a token; the Python reads `.value` there.
pub fn child_token(tree: &Tree, index: usize) -> &Token {
    match tree.children.get(index) {
        Some(Node::Token(child)) => child,
        other => panic!("{} child {index} is {other:?}, not a token", tree.data),
    }
}

/// `_find_stmts_among_children`, shared by basic_checks and if_return_checks: child trees whose rule name
/// ends with `suffix`.
pub fn find_stmts_among_children<'a>(tree: &'a Tree, suffix: &str) -> Vec<&'a Tree> {
    tree.subtrees()
        .filter(|child| child.data.ends_with(suffix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gdscript::Span;

    fn token(value: &str, line: usize) -> Node {
        let span = Span {
            line,
            column: 1,
            end_line: line,
            end_column: Some(2),
            start_pos: 0,
            end_pos: Some(1),
        };
        Node::Token(Token {
            kind: "NAME",
            value: value.into(),
            span,
        })
    }

    fn tree(data: &'static str, children: Vec<Node>) -> Tree {
        Tree {
            data,
            children,
            meta: None,
        }
    }

    #[test]
    fn iter_subtrees_matches_lark_order() {
        // start(a(c, d), b): lark yields c, d, a, b, start.
        let a = tree(
            "a",
            vec![Node::Tree(tree("c", vec![])), Node::Tree(tree("d", vec![]))],
        );
        let start = tree("start", vec![Node::Tree(a), Node::Tree(tree("b", vec![]))]);
        let order: Vec<_> = iter_subtrees(&start).iter().map(|t| t.data).collect();
        assert_eq!(order, ["c", "d", "a", "b", "start"]);
    }

    #[test]
    fn equality_ignores_positions() {
        assert!(nodes_equal(&token("x", 1), &token("x", 9)));
        assert!(!nodes_equal(&token("x", 1), &token("y", 1)));
        assert!(!nodes_equal(&token("x", 1), &Node::Tree(tree("x", vec![]))));
    }
}
