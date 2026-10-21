//! gdformat's safety checks: the formatted code keeps every comment, parses to the same tree up to
//! formatting, and formats to itself.

use gdscript::{Node, Tree};

use crate::comments::gather_comments;
use crate::expression_to_str::expression_to_str;
use crate::formatter::format_code;
use crate::node::Ex;

/// What `check_formatting_safety` found wrong, named as gdtoolkit's exceptions are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyViolation {
    /// The formatted code parses to a different tree. gdtoolkit attaches a diff of the trees, which
    /// gdformat never prints; this doesn't build it.
    TreeInvariantViolation,
    /// Formatting the formatted code changes it again.
    FormattingStabilityViolation,
    /// A comment of the given code is in none of the formatted code's comments.
    CommentPersistenceViolation { missing_comment: String },
}

impl SafetyViolation {
    pub fn name(&self) -> &'static str {
        match self {
            SafetyViolation::TreeInvariantViolation => "TreeInvariantViolation",
            SafetyViolation::FormattingStabilityViolation => "FormattingStabilityViolation",
            SafetyViolation::CommentPersistenceViolation { .. } => "CommentPersistenceViolation",
        }
    }
}

#[derive(Debug)]
pub enum SafetyError<E> {
    /// The formatted code doesn't parse.
    Parse(E),
    Violation(SafetyViolation),
}

/// The code tree and comment tree of some code, as `parser.parse(code, gather_metadata=True)` and
/// `parser.parse_comments(code)` build them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCode {
    pub tree: Tree,
    pub comments: Tree,
}

/// gdtoolkit's `check_formatting_safety`, given the trees of `given_code` and `parse` to build those of
/// `formatted_code`, which runs only when the two differ.
pub fn check_formatting_safety<E>(
    given_code: &str,
    formatted_code: &str,
    max_line_length: i64,
    given_code_trees: &ParsedCode,
    spaces_for_indent: Option<usize>,
    parse: impl FnOnce(&str) -> Result<ParsedCode, E>,
) -> Result<(), SafetyError<E>> {
    if given_code == formatted_code {
        return Ok(());
    }
    let formatted = parse(formatted_code).map_err(SafetyError::Parse)?;
    check_comment_persistence(&given_code_trees.comments, &formatted.comments)
        .and_then(|()| check_tree_invariant(&given_code_trees.tree, &formatted.tree))
        .and_then(|()| {
            check_formatting_stability(
                formatted_code,
                max_line_length,
                &formatted,
                spaces_for_indent,
            )
        })
        .map_err(SafetyError::Violation)
}

pub fn check_tree_invariant(
    given_code_parse_tree: &Tree,
    formatted_code_parse_tree: &Tree,
) -> Result<(), SafetyViolation> {
    match loosen_tree(given_code_parse_tree) == loosen_tree(formatted_code_parse_tree) {
        true => Ok(()),
        false => Err(SafetyViolation::TreeInvariantViolation),
    }
}

pub fn check_formatting_stability(
    formatted_code: &str,
    max_line_length: i64,
    formatted: &ParsedCode,
    spaces_for_indent: Option<usize>,
) -> Result<(), SafetyViolation> {
    let code_formatted_again = format_code(
        formatted_code,
        max_line_length,
        spaces_for_indent,
        &formatted.tree,
        &formatted.comments,
    );
    match formatted_code == code_formatted_again {
        true => Ok(()),
        false => Err(SafetyViolation::FormattingStabilityViolation),
    }
}

pub fn check_comment_persistence(
    given_code_comment_parse_tree: &Tree,
    formatted_code_comment_parse_tree: &Tree,
) -> Result<(), SafetyViolation> {
    let comments_after_formatting = gather_comments(formatted_code_comment_parse_tree);
    for original_comment in gather_comments(given_code_comment_parse_tree) {
        if !comments_after_formatting
            .iter()
            .any(|comment| comment.contains(&original_comment))
        {
            return Err(SafetyViolation::CommentPersistenceViolation {
                missing_comment: original_comment,
            });
        }
    }
    Ok(())
}

/// A tree after gdtoolkit's `LoosenTreeTransformer`: what formatting may change is normalized away.
/// A `Str` is the plain string the transformer puts in place of a string literal; it equals a token
/// with the same text, as a Python `str` equals a lark `Token`.
#[derive(Debug)]
enum Loose {
    Str(String),
    Token {
        kind: &'static str,
        value: String,
    },
    Tree {
        data: &'static str,
        children: Vec<Loose>,
    },
}

impl PartialEq for Loose {
    fn eq(&self, other: &Loose) -> bool {
        match (self, other) {
            (Loose::Str(a), Loose::Str(b)) => a == b,
            (Loose::Str(a), Loose::Token { value, .. })
            | (Loose::Token { value, .. }, Loose::Str(a)) => a == value,
            (
                Loose::Token { kind, value },
                Loose::Token {
                    kind: other_kind,
                    value: other_value,
                },
            ) => kind == other_kind && value == other_value,
            (
                Loose::Tree { data, children },
                Loose::Tree {
                    data: other_data,
                    children: other_children,
                },
            ) => data == other_data && children == other_children,
            _ => false,
        }
    }
}

fn loosen_tree(tree: &Tree) -> Loose {
    let args: Vec<Loose> = tree.children.iter().map(loosen).collect();
    let tree_of = |data: &'static str, children: Vec<Loose>| Loose::Tree { data, children };
    let without_annotations = |args: Vec<Loose>| {
        args.into_iter()
            .filter(|arg| {
                !matches!(
                    arg,
                    Loose::Tree {
                        data: "annotation" | "property_body_def",
                        ..
                    }
                )
            })
            .collect()
    };
    match tree.data {
        "par_expr" | "par_pattern" => args.into_iter().next().expect("empty parentheses"),
        "asless_actual_neg_expr" => match &args[1] {
            Loose::Token {
                kind: "NUMBER",
                value,
            } => Loose::Token {
                kind: "NUMBER",
                value: format!("-{value}"),
            },
            _ => tree_of(tree.data, args),
        },
        "asless_comparison" => tree_of("comparison", args),
        "asless_and_test" => tree_of("and_test", args),
        "asless_or_test" => tree_of("or_test", args),
        "asless_bitw_or" => tree_of("bitw_or", args),
        "asless_bitw_xor" => tree_of("bitw_xor", args),
        "asless_bitw_and" => tree_of("bitw_and", args),
        "asless_shift_expr" => tree_of("shift_expr", args),
        "asless_type_test" => tree_of("type_test", args),
        "asless_content_test" => tree_of("content_test", args),
        "asless_test_expr" => tree_of("test_expr", args),
        "asless_arith_expr" => tree_of("arith_expr", args),
        "asless_mdr_expr" => tree_of("mdr_expr", args),
        "asless_pow_expr" => tree_of("pow_expr", args),
        "string" | "rstring" => {
            let token = tree.children[0].as_token().expect("string without a token");
            Loose::Str(expression_to_str(&Ex::Token(token)))
        }
        "signal_stmt" => {
            let mut args = args;
            if args.len() > 1
                && matches!(&args[1], Loose::Tree { children, .. } if children.is_empty())
            {
                args.pop();
            }
            tree_of("signal_stmt", args)
        }
        "start" | "class_def" => tree_of(tree.data, without_annotations(args)),
        "inline_property_body" => tree_of("inline_property_body", Vec::new()),
        _ => tree_of(tree.data, args),
    }
}

fn loosen(node: &Node) -> Loose {
    match node {
        Node::Tree(tree) => loosen_tree(tree),
        Node::Token(token) => Loose::Token {
            kind: token.kind,
            value: token.value.clone(),
        },
    }
}
