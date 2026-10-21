//! Port of lark 1.2.2's `parse_tree_builder.py` for the chain gdtoolkit's options build
//! (LALR, `maybe_placeholders=False`, `propagate_positions=True`): PropagatePositions around
//! ChildFilterLALR_NoPlaceholders around ExpandSingleChild around the Tree constructor.

use crate::lexer::{LexToken, RawSpan};
use crate::tables::{Grammar, Rule};
use crate::tree::{Node, Span, Token, Tree};

/// A value on the parser's stack: a token as lexed, or a built node with its meta's `container_*`
/// fields, which only PropagatePositions reads. A tree's container is None when its meta is empty.
pub enum Item<'s> {
    Lexed(LexToken<'s>),
    Built(Node, Option<RawSpan>),
}

impl Item<'_> {
    /// `_pp_get_meta` for one child, then `getattr(meta, 'container_…', meta.…)`.
    fn container(&self) -> Option<RawSpan> {
        match self {
            Item::Lexed(token) => Some(token.span),
            Item::Built(Node::Token(token), _) => Some(raw(&token.span)),
            Item::Built(Node::Tree(_), container) => *container,
        }
    }

    pub fn into_node(self, grammar: &Grammar) -> Node {
        match self {
            Item::Lexed(token) => Node::Token(Token {
                kind: grammar.symbols[token.kind as usize],
                value: token.value.to_owned(),
                span: span(&token.span, &token.span),
            }),
            Item::Built(node, _) => node,
        }
    }
}

/// The callback lark builds for `rule`, applied to the values it reduces: pops them off `stack` and
/// pushes the result.
pub fn reduce<'s>(grammar: &Grammar, rule: &Rule, stack: &mut Vec<Item<'s>>) {
    if rule.expand1 && rule.size == 1 && rule.include.is_none() {
        // ExpandSingleChild hands the child back, and PropagatePositions recomputes its container from
        // itself: the value stays as it is. Most reduces are these, down the expression rules.
        return;
    }
    let from = stack.len() - rule.size;
    let first = stack[from..].iter().find_map(Item::container);
    let last = stack[from..].iter().rev().find_map(Item::container);
    let filtered = child_filter(grammar, rule, stack.drain(from..));
    stack.push(build(rule, filtered, first, last));
}

fn build<'s>(
    rule: &Rule,
    filtered: Vec<Node>,
    first: Option<RawSpan>,
    last: Option<RawSpan>,
) -> Item<'s> {
    let mut result = match (rule.expand1, filtered.len()) {
        (true, 1) => filtered.into_iter().next().expect("one child"),
        _ => Node::Tree(Tree {
            data: rule.data,
            children: filtered,
            meta: None,
        }),
    };
    let Node::Tree(tree) = &mut result else {
        return Item::Built(result, None);
    };
    let (Some(first), Some(last)) = (first, last) else {
        return Item::Built(result, None);
    };
    // A tree a `?rule` passed up keeps its own start and end; only its container grows.
    if tree.meta.is_none() {
        tree.meta = Some(span(&first, &last));
    }
    let container = RawSpan {
        line: first.line,
        column: first.column,
        start_pos: first.start_pos,
        end_line: last.end_line,
        end_column: last.end_column,
        end_pos: last.end_pos,
    };
    Item::Built(result, Some(container))
}

/// ChildFilterLALR_NoPlaceholders: keep the included children, splicing in those of `_rule` trees.
fn child_filter<'s>(
    grammar: &Grammar,
    rule: &Rule,
    children: impl Iterator<Item = Item<'s>>,
) -> Vec<Node> {
    let Some(include) = rule.include else {
        return children.map(|c| c.into_node(grammar)).collect();
    };
    let mut filtered = Vec::new();
    let mut wanted = include.iter().peekable();
    for (index, child) in children.enumerate() {
        let Some(&&(at, expand)) = wanted.peek() else {
            break;
        };
        if at as usize != index {
            continue;
        }
        wanted.next();
        match (expand, child.into_node(grammar)) {
            // Optimize for left-recursion, as lark does: `_x_star: _x_star item` reuses the list.
            (true, Node::Tree(inlined)) if filtered.is_empty() => filtered = inlined.children,
            (true, Node::Tree(inlined)) => filtered.extend(inlined.children),
            (true, Node::Token(token)) => {
                panic!("cannot inline token {token:?} into {}", rule.data)
            }
            (false, node) => filtered.push(node),
        }
    }
    filtered
}

fn raw(span: &Span) -> RawSpan {
    RawSpan {
        line: Some(span.line),
        column: Some(span.column),
        end_line: Some(span.end_line),
        end_column: span.end_column,
        start_pos: Some(span.start_pos),
        end_pos: span.end_pos,
    }
}

/// A tree's meta from its first and last children's containers. Only `end_column` and `end_pos` may be
/// missing: the DEDENTs gdtoolkit makes carry a line but no column or position, and they only ever end
/// a rule.
fn span(first: &RawSpan, last: &RawSpan) -> Span {
    let need = |field: Option<usize>, name: &str| {
        field.unwrap_or_else(|| panic!("lark leaves {name} unset here; the tree contract needs it"))
    };
    Span {
        line: need(first.line, "line"),
        column: need(first.column, "column"),
        start_pos: need(first.start_pos, "start_pos"),
        end_line: need(last.end_line, "end_line"),
        end_column: last.end_column,
        end_pos: last.end_pos,
    }
}
