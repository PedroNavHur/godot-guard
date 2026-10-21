//! Score GDScript functions on a gdtoolkit parse tree.
//!
//! Ported from godot-guard's `complexity.py`, itself ported from pedro-best-practices `complexity_core.py`
//! and `complexity_gdscript.py`; the scores must stay identical (tests/oracle_rules.rs).
//!
//! Cognitive complexity follows the SonarSource whitepaper (G. Ann Campbell, v1.7): structural increments
//! cost 1 plus the current nesting, fundamental increments cost 1, nested functions add a nesting level and
//! fold their score into the enclosing top-level function. Cyclomatic complexity follows ESLint's classic
//! `complexity` rule: 1 plus one per decision point, scored for every function on its own. Language mappings:
//! - `elif` is an else-if (+1 flat); `else` is +1 flat.
//! - `match` is one structural increment; each non-wildcard branch is a cyclomatic decision, and a `when`
//!   guard adds 1 to both metrics.
//! - `and`/`&&` and `or`/`||` are the same operators; `not` ends a chain.
//! - Lambdas nest like any nested function, named after the enclosing function: `outer.<lambda#2>` for the
//!   second anonymous one in `outer`. Property getters and setters score as their own functions, named
//!   `property.set` and `property.get`.
//! - Recursion: a call to the function's own name, or `self.` plus that name.

use std::collections::{HashMap, HashSet};

use gdscript::{Node, Tree};

pub const BOOLEAN: [(&str, &str); 4] = [
    ("or_test", "or"),
    ("asless_or_test", "or"),
    ("and_test", "and"),
    ("asless_and_test", "and"),
];
pub const TERNARY: [&str; 2] = ["test_expr", "asless_test_expr"];
pub const LOOPS: [&str; 3] = ["for_stmt", "for_stmt_typed", "while_stmt"];

const ANONYMOUS: &str = "<anonymous>";

fn is_boolean(data: &str) -> bool {
    BOOLEAN.iter().any(|(rule, _)| *rule == data)
}

fn is_ternary(data: &str) -> bool {
    TERNARY.contains(&data)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub line: usize,
    pub end_line: usize,
    pub cyclomatic: usize,
    /// None for nested functions: counted in the top-level one.
    pub cognitive: Option<usize>,
}

struct Frame {
    /// Index into `Scorer::functions`.
    function: usize,
    nesting: usize,
    recursion_counted: bool,
    own_names: Vec<String>,
}

#[derive(Default)]
pub struct Scorer {
    pub functions: Vec<Function>,
    frames: Vec<Frame>,
}

impl Scorer {
    pub fn active(&self) -> bool {
        !self.frames.is_empty()
    }

    /// Start scoring a function body; `end_function` closes it. `own_names` are the call forms that count
    /// as recursion.
    pub fn function(&mut self, name: String, line: usize, end_line: usize, own_names: Vec<String>) {
        let (nesting, cognitive) = match self.frames.last() {
            Some(frame) => (frame.nesting + 1, None),
            None => (0, Some(0)),
        };
        self.functions.push(Function {
            name,
            line,
            end_line,
            cyclomatic: 1,
            cognitive,
        });
        let function = self.functions.len() - 1;
        self.frames.push(Frame {
            function,
            nesting,
            recursion_counted: false,
            own_names,
        });
    }

    pub fn end_function(&mut self) {
        self.frames.pop();
    }

    /// Enter (`entering`) or leave a nesting region: the body of an if, else, loop, case, or ternary branch.
    fn nest(&mut self, entering: bool) {
        if let Some(frame) = self.frames.last_mut() {
            match entering {
                true => frame.nesting += 1,
                false => frame.nesting -= 1,
            }
        }
    }

    pub fn enclosing(&self) -> &str {
        self.frames
            .last()
            .map_or("", |frame| &self.functions[frame.function].name)
    }

    fn top_cognitive(&mut self) -> &mut usize {
        let top = self.frames[0].function;
        self.functions[top]
            .cognitive
            .as_mut()
            .expect("a top-level function has a cognitive score")
    }

    /// if, ternary, match, loop: 1 plus the nesting level.
    pub fn structural(&mut self) {
        if let Some(frame) = self.frames.last() {
            let cost = 1 + frame.nesting;
            *self.top_cognitive() += cost;
        }
    }

    /// else, elif, operator runs, recursion: a flat 1 each.
    pub fn fundamental(&mut self, amount: usize) {
        if self.active() && amount > 0 {
            *self.top_cognitive() += amount;
        }
    }

    /// A cyclomatic decision point in the innermost function.
    pub fn decision(&mut self, amount: usize) {
        if let Some(frame) = self.frames.last() {
            self.functions[frame.function].cyclomatic += amount;
        }
    }

    /// Count recursion once per function when `form` names the function itself.
    pub fn call(&mut self, form: &str) {
        let Some(frame) = self.frames.last_mut() else {
            return;
        };
        if !frame.recursion_counted && frame.own_names.iter().any(|own| own == form) {
            frame.recursion_counted = true;
            self.fundamental(1);
        }
    }
}

/// Runs of like operators in a flattened boolean chain: `a and b or c` is 2.
pub fn operator_runs(operators: &[&str]) -> usize {
    let mut runs = 0;
    let mut previous = None;
    for operator in operators {
        if previous != Some(operator) {
            runs += 1;
            previous = Some(operator);
        }
    }
    runs
}

/// A token's text; "" for a tree.
pub fn token_text(node: &Node) -> &str {
    node.as_token().map_or("", |token| token.value.as_str())
}

/// The last line any token below `tree` reaches; the tree's own line when it holds none.
pub fn end_line(tree: &Tree) -> usize {
    let mut last = None;
    for node in tree.iter_topdown() {
        for token in node.children.iter().filter_map(Node::as_token) {
            let line = match token.span.end_line {
                0 => token.span.line,
                end => end,
            };
            last = last.max(Some(line));
        }
    }
    last.unwrap_or_else(|| line(tree))
}

/// The tree's first line, as lark's `meta.line` reports it.
pub fn line(tree: &Tree) -> usize {
    tree.meta
        .expect("a tree that holds no token has no line")
        .line
}

/// The first NAME token among the tree's children, or `<anonymous>`.
pub fn first_name(tree: &Tree) -> &str {
    tree.children
        .iter()
        .filter_map(Node::as_token)
        .find(|token| token.kind == "NAME")
        .map_or(ANONYMOUS, |token| token.value.as_str())
}

/// The child tree at `index`, where the grammar puts one; the Python raises without one, and so does this.
pub fn child_tree(tree: &Tree, index: usize) -> &Tree {
    tree.children[index]
        .as_tree()
        .unwrap_or_else(|| panic!("{}: child {index} is not a tree", tree.data))
}

pub fn is_wildcard(pattern: &Node) -> bool {
    pattern.as_tree().is_some_and(|tree| {
        tree.subtrees()
            .any(|child| child.data == "wildcard_pattern")
    })
}

pub struct Analyzer<'a> {
    pub scorer: Scorer,
    seen_boolean: HashSet<*const Tree>,
    classes: Vec<&'a str>,
    last_var: String,
    lambdas: HashMap<String, usize>,
}

impl<'a> Analyzer<'a> {
    pub fn new() -> Self {
        Analyzer {
            scorer: Scorer::default(),
            seen_boolean: HashSet::new(),
            classes: Vec::new(),
            last_var: String::new(),
            lambdas: HashMap::new(),
        }
    }

    pub fn walk(&mut self, node: &'a Node) {
        if let Node::Tree(tree) = node {
            self.walk_tree(tree);
        }
    }

    fn walk_tree(&mut self, node: &'a Tree) {
        match node.data {
            "class_def" => self.visit_class_def(node),
            "class_var_stmt" => self.visit_class_var_stmt(node),
            "func_def" => self.visit_func_def(node),
            "property_custom_setter" => self.visit_property_custom_setter(node),
            "property_custom_getter" => self.visit_property_custom_getter(node),
            "lambda" => self.visit_lambda(node),
            "standalone_call" => self.visit_standalone_call(node),
            "getattr_call" => self.visit_getattr_call(node),
            "if_stmt" => self.visit_if_stmt(node),
            "match_stmt" => self.visit_match_stmt(node),
            data if LOOPS.contains(&data) => self.visit_loop(node),
            data if is_boolean(data) => self.visit_boolean(node),
            data if is_ternary(data) => self.visit_ternary(node),
            _ => self.children(node),
        }
    }

    fn children(&mut self, node: &'a Tree) {
        for child in &node.children {
            self.walk(child);
        }
    }

    fn nested_children<I: IntoIterator<Item = &'a Node>>(&mut self, children: I) {
        self.scorer.nest(true);
        for child in children {
            self.walk(child);
        }
        self.scorer.nest(false);
    }

    // Functions and classes

    fn visit_class_def(&mut self, node: &'a Tree) {
        self.classes.push(first_name(node));
        self.children(node);
        self.classes.pop();
    }

    fn visit_class_var_stmt(&mut self, node: &'a Tree) {
        self.last_var = first_name(child_tree(node, 0)).to_owned();
        self.children(node);
    }

    fn function(&mut self, node: &'a Tree, name: &str, body: &'a [Node]) {
        let in_class = !self.scorer.active();
        let qualified = match in_class && !self.classes.is_empty() {
            true => format!("{}.{name}", self.classes.join(".")),
            false => name.to_owned(),
        };
        let own = vec![name.to_owned(), format!("self.{name}")];
        self.scorer
            .function(qualified, line(node), end_line(node), own);
        for child in body {
            self.walk(child);
        }
        self.scorer.end_function();
    }

    fn visit_func_def(&mut self, node: &'a Tree) {
        let name = first_name(child_tree(node, 0));
        self.function(node, name, &node.children[1..]);
    }

    fn visit_property_custom_setter(&mut self, node: &'a Tree) {
        let name = format!("{}.set", self.last_var);
        self.function(node, &name, &node.children);
    }

    fn visit_property_custom_getter(&mut self, node: &'a Tree) {
        let name = format!("{}.get", self.last_var);
        self.function(node, &name, &node.children);
    }

    fn visit_lambda(&mut self, node: &'a Tree) {
        let name = first_name(child_tree(node, 0));
        let qualified = self.lambda_name(name);
        self.scorer
            .function(qualified, line(node), end_line(node), vec![name.to_owned()]);
        for child in &node.children[1..] {
            self.walk(child);
        }
        self.scorer.end_function();
    }

    /// `outer.name`, or `outer.<lambda#N>` numbered within `outer` for an anonymous lambda.
    fn lambda_name(&mut self, name: &str) -> String {
        let enclosing = match self.scorer.enclosing() {
            "" => self.classes.join("."),
            found => found.to_owned(),
        };
        let name = match name {
            ANONYMOUS => {
                let count = self.lambdas.entry(enclosing.clone()).or_insert(0);
                *count += 1;
                format!("<lambda#{count}>")
            }
            named => named.to_owned(),
        };
        match enclosing.is_empty() {
            true => name,
            false => format!("{enclosing}.{name}"),
        }
    }

    fn visit_standalone_call(&mut self, node: &'a Tree) {
        self.scorer.call(first_name(node));
        self.children(node);
    }

    fn visit_getattr_call(&mut self, node: &'a Tree) {
        if let Some(target) = node.children[0]
            .as_tree()
            .filter(|target| target.data == "getattr")
        {
            let names: Vec<&str> = target
                .children
                .iter()
                .filter_map(Node::as_token)
                .filter(|token| token.kind == "NAME")
                .map(|token| token.value.as_str())
                .collect();
            if names.len() == 2 {
                self.scorer.call(&names.join("."));
            }
        }
        self.children(node);
    }

    // Branches

    fn visit_if_stmt(&mut self, node: &'a Tree) {
        for branch in node.subtrees() {
            match branch.data {
                "if_branch" => self.scorer.structural(),
                _ => self.scorer.fundamental(1),
            }
            if branch.data == "else_branch" {
                self.nested_children(&branch.children);
                continue;
            }
            self.scorer.decision(1);
            self.walk(&branch.children[0]);
            self.nested_children(&branch.children[1..]);
        }
    }

    fn visit_loop(&mut self, node: &'a Tree) {
        self.scorer.structural();
        self.scorer.decision(1);
        let children: Vec<&'a Node> = node
            .children
            .iter()
            .filter(|child| child.as_tree().is_some())
            .collect();
        let header = children
            .iter()
            .position(|child| child.as_tree().is_some_and(|tree| tree.data == "expr"))
            .unwrap_or(0);
        let split = (header + 1).min(children.len());
        for child in &children[..split] {
            self.walk(child);
        }
        self.nested_children(children[split..].iter().copied());
    }

    fn visit_match_stmt(&mut self, node: &'a Tree) {
        self.scorer.structural();
        self.walk(&node.children[0]);
        for branch in node.children[1..].iter().filter_map(Node::as_tree) {
            self.match_branch(branch);
        }
    }

    fn match_branch(&mut self, branch: &'a Tree) {
        let wildcard = is_wildcard(&branch.children[0]);
        let mut rest = &branch.children[1..];
        if branch.data == "guarded_match_branch" {
            let guard = &rest[0];
            rest = &rest[1..];
            self.scorer.decision(if wildcard { 1 } else { 2 });
            self.scorer.fundamental(1);
            self.walk(guard);
        } else if !wildcard {
            self.scorer.decision(1);
        }
        self.nested_children(rest);
    }

    // Expressions

    fn visit_ternary(&mut self, node: &'a Tree) {
        let parts: Vec<&'a Node> = node
            .children
            .iter()
            .filter(|child| {
                child.as_tree().is_some() || !matches!(token_text(child), "if" | "else")
            })
            .collect();
        let [value, condition, alternative] = parts[..] else {
            self.children(node);
            return;
        };
        self.scorer.structural();
        self.scorer.decision(1);
        self.walk(condition);
        self.nested_children([value, alternative]);
    }

    fn flatten(&mut self, node: &Tree, operators: &mut Vec<&'static str>) {
        if !is_boolean(node.data) {
            return;
        }
        self.seen_boolean.insert(node as *const Tree);
        for child in &node.children {
            match child {
                Node::Tree(tree) => self.flatten(tree, operators),
                Node::Token(token) => match token.value.as_str() {
                    "or" | "||" => operators.push("or"),
                    "and" | "&&" => operators.push("and"),
                    _ => {}
                },
            }
        }
    }

    fn visit_boolean(&mut self, node: &'a Tree) {
        let own = node
            .children
            .iter()
            .filter(|child| {
                child.as_token().is_some()
                    && matches!(token_text(child), "or" | "||" | "and" | "&&")
            })
            .count();
        if !self.seen_boolean.contains(&(node as *const Tree)) {
            let mut operators = Vec::new();
            self.flatten(node, &mut operators);
            self.scorer.fundamental(operator_runs(&operators));
        }
        self.scorer.decision(own);
        self.children(node);
    }
}

impl Default for Analyzer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn score(tree: &Tree) -> Vec<Function> {
    let mut analyzer = Analyzer::new();
    analyzer.walk_tree(tree);
    analyzer.scorer.functions
}

/// Lines of each outermost conditional expression that holds another one.
pub fn nested_ternaries(tree: &Tree) -> Vec<usize> {
    let mut found = Vec::new();
    let mut pending = vec![tree];
    while let Some(node) = pending.pop() {
        if !is_ternary(node.data) {
            pending.extend(node.subtrees());
            continue;
        }
        if node
            .iter_topdown()
            .skip(1)
            .any(|inner| is_ternary(inner.data))
        {
            found.push(line(node));
        }
    }
    found.sort();
    found
}
