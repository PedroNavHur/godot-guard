//! Syntax-pattern rules on a gdtoolkit parse tree: hot-path work, private exports, and self-assignment.
//!
//! A hot function is a callback Godot runs every frame or input event; a frame function, every frame.
//! Lambdas inside one are skipped, since they run when called. SceneTree and MainLoop scripts are test and
//! tool runners, not game code, and are exempt from the hot-path rules.

use std::ops::Index;

use gdscript::{Node, Tree};

use crate::complexity::{child_tree, first_name, line};
use crate::interface::is_export_annotation;

pub const FRAME_FUNCTIONS: [&str; 2] = ["_process", "_physics_process"];
pub const HOT_FUNCTIONS: [&str; 7] = [
    "_process",
    "_physics_process",
    "_input",
    "_unhandled_input",
    "_unhandled_key_input",
    "_gui_input",
    "_shortcut_input",
];
pub const LOOKUP_CALLS: [&str; 7] = [
    "get_node",
    "get_node_or_null",
    "find_child",
    "find_children",
    "find_parent",
    "get_nodes_in_group",
    "get_first_node_in_group",
];
pub const NODE_PATHS: [(&str, &str); 2] = [("get_node", "$path"), ("unique_node_path", "%name")];
pub const LITERALS: [(&str, &str); 2] = [
    ("array", "an array literal"),
    ("dict", "a dictionary literal"),
];
pub const RUNNERS: [&str; 2] = ["SceneTree", "MainLoop"];
pub const EXTENDS_STATEMENTS: [&str; 2] = ["extends_stmt", "classname_extends_stmt"];
pub const HOT_RULES: [&str; 3] = [
    "lookup-in-hot-path",
    "load-in-hot-path",
    "per-frame-allocation",
];
/// Hidden from the inspector, so a private name is their purpose.
pub const STORAGE_EXPORTS: [&str; 1] = ["export_storage"];

/// Baselined per file, counted like long-comment: rule -> the noun its count labels.
pub const COUNTED: [(&str, &str); 4] = [
    ("lookup-in-hot-path", "hot-path lookups"),
    ("load-in-hot-path", "hot-path loads"),
    ("private-export", "private exports"),
    ("self-assign", "self-assignments"),
];
pub const ADVISORY: [&str; 1] = ["per-frame-allocation"];

pub type Hit = (usize, String);

/// (line, message) hits by rule, in the order `scan` produces them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hits(Vec<(&'static str, Vec<Hit>)>);

impl Hits {
    pub fn get(&self, rule: &str) -> Option<&[Hit]> {
        self.0
            .iter()
            .find(|(name, _)| *name == rule)
            .map(|(_, hits)| hits.as_slice())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &[Hit])> {
        self.0.iter().map(|(rule, hits)| (*rule, hits.as_slice()))
    }

    fn entry(&mut self, rule: &'static str) -> &mut Vec<Hit> {
        let index = match self.0.iter().position(|(name, _)| *name == rule) {
            Some(index) => index,
            None => {
                self.0.push((rule, Vec::new()));
                self.0.len() - 1
            }
        };
        &mut self.0[index].1
    }
}

impl Index<&str> for Hits {
    type Output = [Hit];

    /// The rule's hits; panics on a rule `scan` does not produce, as the Python's dict raises KeyError.
    fn index(&self, rule: &str) -> &[Hit] {
        self.get(rule)
            .unwrap_or_else(|| panic!("no hits for rule {rule}"))
    }
}

fn lookup<'a>(table: &[(&str, &'a str)], key: &str) -> Option<&'a str> {
    table
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| *value)
}

/// The function or method a call node names: `f` in `f()` and `a.b.f()`; "" for anything else.
pub fn called_name(node: &Tree) -> &str {
    if node.data == "standalone_call" {
        return first_name(node);
    }
    let target = match node.data {
        "getattr_call" => node.children[0].as_tree(),
        _ => None,
    };
    let Some(target) = target.filter(|target| target.data == "getattr") else {
        return "";
    };
    target
        .children
        .iter()
        .filter_map(Node::as_token)
        .rfind(|token| token.kind == "NAME")
        .map_or("", |token| token.value.as_str())
}

/// (rule, message) for work a hot function should not repeat each call.
pub fn hot_hit(node: &Tree, function: &str) -> Option<(&'static str, String)> {
    let called = called_name(node);
    let path = lookup(&NODE_PATHS, node.data);
    if path.is_some() || LOOKUP_CALLS.contains(&called) {
        let what = path.map_or_else(|| format!("{called}()"), str::to_owned);
        let message = format!(
            "{what} inside {function}(): resolve it once into an @onready var or in _ready()"
        );
        return Some(("lookup-in-hot-path", message));
    }
    if called == "load" {
        let message = format!(
            "load() inside {function}() reads the resource each call: preload it or load it once"
        );
        return Some(("load-in-hot-path", message));
    }
    let what = lookup(&LITERALS, node.data).or((called == "new").then_some(".new()"))?;
    if !FRAME_FUNCTIONS.contains(&function) {
        return None;
    }
    let message = format!(
        "{what} inside {function}() allocates every frame; reuse one if it does not change"
    );
    Some(("per-frame-allocation", message))
}

/// The child subtrees of `node`; none for a `$` path, so `$%name` counts as one lookup.
pub fn subtrees(node: &Tree) -> Vec<&Tree> {
    if node.data == "get_node" {
        return Vec::new();
    }
    node.subtrees().collect()
}

/// Every subtree below `node`, without descending into lambdas or `$` paths.
pub fn body_nodes(node: &Tree) -> Vec<&Tree> {
    let mut found = Vec::new();
    let mut pending = subtrees(node);
    while let Some(current) = pending.pop() {
        if current.data == "lambda" {
            continue;
        }
        found.push(current);
        pending.extend(subtrees(current));
    }
    found
}

/// (name, func_def) for each hot function; none in a runner script.
pub fn hot_functions(tree: &Tree) -> Vec<(&str, &Tree)> {
    if RUNNERS.contains(&extends(tree).as_str()) {
        return Vec::new();
    }
    tree.iter_topdown()
        .filter(|node| node.data == "func_def")
        .map(|function| (first_name(child_tree(function, 0)), function))
        .filter(|(name, _)| HOT_FUNCTIONS.contains(name))
        .collect()
}

pub fn hot_hits(tree: &Tree) -> Hits {
    let mut found = Hits(HOT_RULES.iter().map(|rule| (*rule, Vec::new())).collect());
    for (name, function) in hot_functions(tree) {
        for node in body_nodes(function) {
            if let Some((rule, message)) = hot_hit(node, name) {
                found.entry(rule).push((line(node), message));
            }
        }
    }
    found
}

pub fn is_export(annotation: &Tree) -> bool {
    // A tree's str() never starts with "export", so a tree child reads like no name.
    let name = annotation
        .children
        .first()
        .and_then(Node::as_token)
        .map_or("", |token| token.value.as_str());
    is_export_annotation(name) && !STORAGE_EXPORTS.contains(&name)
}

/// `class_var_stmt` nodes in one class body that carry an @export annotation.
pub fn exported_variables(body: &Tree) -> Vec<&Tree> {
    let mut exported = false;
    let mut found = Vec::new();
    for child in body.subtrees() {
        if child.data == "annotation" {
            exported = exported || is_export(child);
            continue;
        }
        if exported && child.data == "class_var_stmt" {
            found.push(child);
        }
        exported = false;
    }
    found
}

pub fn private_exports(tree: &Tree) -> Vec<Hit> {
    let bodies =
        std::iter::once(tree).chain(tree.iter_topdown().filter(|node| node.data == "class_def"));
    bodies
        .flat_map(exported_variables)
        .map(|variable| (line(variable), first_name(child_tree(variable, 0))))
        .filter(|(_, name)| name.starts_with('_'))
        .map(|(line, name)| {
            (
                line,
                format!("@export on {name}: the underscore marks it private; drop one of them"),
            )
        })
        .collect()
}

pub fn source_text(node: &Node) -> String {
    match node {
        Node::Token(token) => token.value.clone(),
        Node::Tree(tree) => {
            let mut text = String::new();
            push_tokens(tree, &mut text);
            text
        }
    }
}

fn push_tokens(tree: &Tree, text: &mut String) {
    for child in &tree.children {
        match child {
            Node::Token(token) => text.push_str(&token.value),
            Node::Tree(inner) => push_tokens(inner, text),
        }
    }
}

/// Lark's `==`: trees by rule and children, tokens by type and text; positions do not count.
pub fn same(a: &Node, b: &Node) -> bool {
    match (a, b) {
        (Node::Token(a), Node::Token(b)) => a.kind == b.kind && a.value == b.value,
        (Node::Tree(a), Node::Tree(b)) => {
            a.data == b.data
                && a.children.len() == b.children.len()
                && a.children.iter().zip(&b.children).all(|(a, b)| same(a, b))
        }
        _ => false,
    }
}

pub fn self_assigns(tree: &Tree) -> Vec<Hit> {
    let mut found = Vec::new();
    for node in tree
        .iter_topdown()
        .filter(|node| node.data == "assnmnt_expr")
    {
        let [target, operator, value] = &node.children[..] else {
            continue;
        };
        let assigns = operator.as_token().is_some_and(|token| token.value == "=");
        if assigns && same(target, value) {
            found.push((
                line(node),
                format!("{} is assigned to itself", source_text(target)),
            ));
        }
    }
    found
}

/// The base in the script's own `extends` or `class_name X extends` line; "" without one.
pub fn extends(tree: &Tree) -> String {
    let statement = tree
        .subtrees()
        .find(|child| EXTENDS_STATEMENTS.contains(&child.data));
    statement.map_or_else(String::new, |statement| {
        source_text(
            statement
                .children
                .last()
                .expect("an extends statement names its base"),
        )
    })
}

/// (line, message) hits by rule, for every rule in COUNTED and ADVISORY, sorted by line.
pub fn scan(tree: &Tree) -> Hits {
    let mut found = hot_hits(tree);
    *found.entry("private-export") = private_exports(tree);
    *found.entry("self-assign") = self_assigns(tree);
    for (_, hits) in &mut found.0 {
        hits.sort();
    }
    found
}
