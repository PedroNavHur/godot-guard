//! The nodes the formatter reads: the parser's trees and tokens, and the fake trees gdtoolkit builds on the
//! fly (`dot_chain`, `comma_separated_list`, ...) with a fake `Meta` that holds only line numbers.
//!
//! Lines are `i64` because the Python passes -1 as "no line" and slices with it.

use std::rc::Rc;

use gdscript::{Node, Token, Tree};

#[derive(Clone, Debug)]
pub enum Ex<'a> {
    Token(&'a Token),
    Tree(&'a Tree),
    Fake(Rc<Fake<'a>>),
}

#[derive(Debug)]
pub struct Fake<'a> {
    pub data: &'static str,
    pub children: Vec<Ex<'a>>,
    pub line: i64,
    /// None where the Python's fake meta sets only `line`.
    pub end_line: Option<i64>,
}

impl<'a> From<&'a Node> for Ex<'a> {
    fn from(node: &'a Node) -> Self {
        match node {
            Node::Tree(tree) => Ex::Tree(tree),
            Node::Token(token) => Ex::Token(token),
        }
    }
}

impl<'a> Ex<'a> {
    pub fn fake(data: &'static str, children: Vec<Ex<'a>>, line: i64, end_line: i64) -> Self {
        Ex::Fake(Rc::new(Fake {
            data,
            children,
            line,
            end_line: Some(end_line),
        }))
    }

    pub fn is_token(&self) -> bool {
        matches!(self, Ex::Token(_))
    }

    pub fn token(&self) -> Option<&'a Token> {
        match self {
            Ex::Token(token) => Some(token),
            _ => None,
        }
    }

    /// The rule name; a token has none, as lark's `Token.data` raises.
    pub fn data(&self) -> &'static str {
        match self {
            Ex::Tree(tree) => tree.data,
            Ex::Fake(fake) => fake.data,
            Ex::Token(token) => panic!("token {} has no data", token.kind),
        }
    }

    /// A tree named `name`; false for tokens.
    pub fn is(&self, name: &str) -> bool {
        !self.is_token() && self.data() == name
    }

    /// The token's type, or None for a tree.
    pub fn kind(&self) -> Option<&'static str> {
        self.token().map(|token| token.kind)
    }

    /// The token's text; a tree has none, as lark's `Tree.value` raises.
    pub fn value(&self) -> &'a str {
        match self {
            Ex::Token(token) => &token.value,
            _ => panic!("tree {} has no value", self.data()),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Ex::Tree(tree) => tree.children.len(),
            Ex::Fake(fake) => fake.children.len(),
            Ex::Token(token) => panic!("token {} has no children", token.kind),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Child `index`, negative from the end as in Python.
    pub fn child(&self, index: isize) -> Ex<'a> {
        let len = self.len() as isize;
        let at = if index < 0 { len + index } else { index };
        assert!(
            (0..len).contains(&at),
            "child {index} of {} out of range",
            self.data()
        );
        match self {
            Ex::Tree(tree) => Ex::from(&tree.children[at as usize]),
            Ex::Fake(fake) => fake.children[at as usize].clone(),
            Ex::Token(_) => unreachable!(),
        }
    }

    /// The children from `start` on, like Python's `children[start:]`.
    pub fn children_from(&self, start: usize) -> Vec<Ex<'a>> {
        match self {
            Ex::Tree(tree) => tree.children.iter().skip(start).map(Ex::from).collect(),
            Ex::Fake(fake) => fake.children.iter().skip(start).cloned().collect(),
            Ex::Token(token) => panic!("token {} has no children", token.kind),
        }
    }

    pub fn children(&self) -> Vec<Ex<'a>> {
        self.children_from(0)
    }

    /// `get_line`: the tree's `meta.line` or the token's line.
    pub fn line(&self) -> i64 {
        match self {
            Ex::Token(token) => token.span.line as i64,
            Ex::Tree(tree) => tree.meta.expect("tree without meta has no line").line as i64,
            Ex::Fake(fake) => fake.line,
        }
    }

    /// `get_end_line`: the tree's `meta.end_line` or the token's end line.
    pub fn end_line(&self) -> i64 {
        match self {
            Ex::Token(token) => token.span.end_line as i64,
            Ex::Tree(tree) => {
                tree.meta
                    .expect("tree without meta has no end line")
                    .end_line as i64
            }
            Ex::Fake(fake) => fake.end_line.expect("fake meta has no end line"),
        }
    }
}
