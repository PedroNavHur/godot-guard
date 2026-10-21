//! `name_checks.py`: names checked against the config's patterns.

use gdscript::{Node, Token, Tree};

use crate::config::{Config, NAME_CHECKS};
use crate::problem::Problem;
use crate::utils::{child_tree, find_name_token_among_children};

/// Which names each check reads, in `NAME_CHECKS` order, and its description.
#[derive(Clone, Copy)]
enum Source {
    /// The name tokens of these rules, in the order the Python concatenates them.
    Rules(&'static [&'static str]),
    /// The name tokens of `rule` whose statement does, or does not, assign a call to one of `callees`.
    Filtered {
        rule: &'static str,
        callees: &'static [&'static str],
        wanted: bool,
    },
}

const LOADS: &[&str] = &["load", "preload"];

const SOURCES: [(Source, &str); 14] = [
    (
        Source::Rules(&["func_def"]),
        "Function name \"{}\" is not valid",
    ),
    (
        Source::Rules(&["class_def"]),
        "Class name \"{}\" is not valid",
    ),
    (
        Source::Rules(&["classname_stmt"]),
        "Class name \"{}\" is not valid",
    ),
    (
        Source::Rules(&["signal_stmt"]),
        "Signal name \"{}\" is not valid",
    ),
    (
        Source::Rules(&["enum_named"]),
        "Enum name \"{}\" is not valid",
    ),
    (
        Source::Rules(&["enum_element"]),
        "Enum element name \"{}\" is not valid",
    ),
    (
        Source::Rules(&["for_stmt", "for_stmt_typed"]),
        "Loop variable name \"{}\" is not valid",
    ),
    (
        Source::Rules(&["func_arg_regular", "func_arg_inf", "func_arg_typed"]),
        "Function argument name \"{}\" is not valid",
    ),
    (
        Source::Filtered {
            rule: "func_var_stmt",
            callees: LOADS,
            wanted: false,
        },
        "Function-scope variable name \"{}\" is not valid",
    ),
    (
        Source::Filtered {
            rule: "func_var_stmt",
            callees: &["preload"],
            wanted: true,
        },
        "Function-scope preload variable name \"{}\" is not valid",
    ),
    (
        Source::Filtered {
            rule: "const_stmt",
            callees: LOADS,
            wanted: false,
        },
        "Constant name \"{}\" is not valid",
    ),
    (
        Source::Filtered {
            rule: "const_stmt",
            callees: LOADS,
            wanted: true,
        },
        "Constant (load/preload) name \"{}\" is not valid",
    ),
    (
        Source::Filtered {
            rule: "class_var_stmt",
            callees: LOADS,
            wanted: false,
        },
        "Class-scope variable name \"{}\" is not valid",
    ),
    (
        Source::Filtered {
            rule: "class_var_stmt",
            callees: LOADS,
            wanted: true,
        },
        "Class-scope load/preload variable name \"{}\" is not valid",
    ),
];

pub fn lint(subtrees: &[&Tree], config: &Config) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (check, (source, description)) in NAME_CHECKS.into_iter().zip(SOURCES) {
        if !config.is_enabled(check) {
            continue;
        }
        let pattern = config.name_pattern(check);
        for name_token in name_tokens(subtrees, source) {
            if !pattern.is_match(&name_token.value) {
                problems.push(Problem {
                    name: check,
                    description: description.replacen("{}", &name_token.value, 1),
                    line: name_token.span.line,
                    column: name_token.span.column,
                });
            }
        }
    }
    problems
}

fn name_tokens<'a>(subtrees: &[&'a Tree], source: Source) -> Vec<&'a Token> {
    match source {
        Source::Rules(rules) => rules
            .iter()
            .flat_map(|rule| gather_rule_name_tokens(subtrees, rule, |_| true))
            .collect(),
        Source::Filtered {
            rule,
            callees,
            wanted,
        } => gather_rule_name_tokens(subtrees, rule, |tree| {
            has_call_expr_name_in(tree, callees) == wanted
        }),
    }
}

/// `_gather_rule_name_tokens` for one rule: the name among a node's children, or else among its first
/// child's, where the predicate also looks.
fn gather_rule_name_tokens<'a>(
    subtrees: &[&'a Tree],
    rule: &str,
    predicate: impl Fn(&Tree) -> bool,
) -> Vec<&'a Token> {
    let mut tokens = Vec::new();
    for node in subtrees.iter().filter(|node| node.data == rule) {
        let (name_token, holder) = match find_name_token_among_children(node) {
            Some(token) => (Some(token), *node),
            None => {
                let first = child_tree(node, 0);
                (find_name_token_among_children(first), first)
            }
        };
        let name_token = name_token.unwrap_or_else(|| panic!("{rule} has no name token"));
        if predicate(holder) {
            tokens.push(name_token);
        }
    }
    tokens
}

/// `_has_call_expr_name_in`: whether the first `expr` child that is a bare call calls one of `legal_names`.
fn has_call_expr_name_in(tree: &Tree, legal_names: &[&str]) -> bool {
    let first_call = tree
        .subtrees()
        .filter(|child| child.data == "expr")
        .find_map(|expr| match expr.children.as_slice() {
            [Node::Tree(call)] if call.data == "standalone_call" => Some(call),
            _ => None,
        });
    first_call.is_some_and(|call| {
        let name_token = find_name_token_among_children(call).expect("a call has a name");
        legal_names.contains(&name_token.value.as_str())
    })
}
