//! The parse tree, shaped like the lark tree gdtoolkit 4.5.0 builds, so ports of its consumers read the same.
//!
//! Rule and token names are interned `&'static str`s. Positions follow lark: 1-based lines and columns,
//! and `start_pos`/`end_pos` count code points, not bytes.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Where a tree or token sits in the source, as lark reports it.
///
/// `end_column` and `end_pos` are None where lark leaves them None: a tree whose last token is a DEDENT
/// the indenter made, which carries a line but no column or position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: Option<usize>,
    pub start_pos: usize,
    pub end_pos: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// The terminal's name, such as `NAME` or `__ANON_3`.
    pub kind: &'static str,
    pub value: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tree {
    /// The rule's name after lark's inlining and aliasing, such as `func_def`.
    pub data: &'static str,
    pub children: Vec<Node>,
    /// None where lark's `meta.empty` is true: the tree holds no token.
    pub meta: Option<Span>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Tree(Tree),
    Token(Token),
}

impl Node {
    pub fn as_tree(&self) -> Option<&Tree> {
        match self {
            Node::Tree(tree) => Some(tree),
            Node::Token(_) => None,
        }
    }

    pub fn as_token(&self) -> Option<&Token> {
        match self {
            Node::Token(token) => Some(token),
            Node::Tree(_) => None,
        }
    }
}

impl Tree {
    /// Child trees, in order.
    pub fn subtrees(&self) -> impl Iterator<Item = &Tree> {
        self.children.iter().filter_map(Node::as_tree)
    }

    /// This tree and every tree below it, parents before children, like lark's `iter_subtrees_topdown`.
    pub fn iter_topdown(&self) -> TopDown<'_> {
        TopDown { stack: vec![self] }
    }
}

pub struct TopDown<'a> {
    stack: Vec<&'a Tree>,
}

impl<'a> Iterator for TopDown<'a> {
    type Item = &'a Tree;

    fn next(&mut self) -> Option<&'a Tree> {
        let tree = self.stack.pop()?;
        self.stack
            .extend(tree.subtrees().collect::<Vec<_>>().into_iter().rev());
        Some(tree)
    }
}

/// The `&'static str` for `name`, leaked once per distinct name. For names not known at compile time,
/// such as those read from oracle dumps; the parser uses literals.
pub fn intern(name: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut names = NAMES
        .get_or_init(Default::default)
        .lock()
        .expect("intern table poisoned");
    if let Some(found) = names.get(name) {
        return found;
    }
    let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.insert(leaked);
    leaked
}
