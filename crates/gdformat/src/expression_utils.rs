use crate::node::Ex;
use pycompat::{builtins, string};

pub fn remove_outer_parentheses<'a>(expression: &Ex<'a>) -> Ex<'a> {
    if expression.is("par_expr") {
        return remove_outer_parentheses(&expression.child(0));
    }
    expression.clone()
}

const NON_FOLDABLE: [&str; 12] = [
    "string",
    "rstring",
    "get_node",
    "node_path",
    "string_name",
    "unique_node_path",
    "signal_arg_typed",
    "signal_arg_regular",
    "non_foldable_dot_chain",
    "var_capture_pattern",
    "etc_pattern",
    "wildcard_pattern",
];

pub fn is_foldable(expression: &Ex) -> bool {
    if is_multiline_string(expression) {
        return true;
    }
    !expression.is_token() && !NON_FOLDABLE.contains(&expression.data())
}

pub fn has_trailing_comma(expression: &Ex) -> bool {
    !expression.is_token() && !expression.is_empty() && expression.child(-1).is("trailing_comma")
}

pub fn is_trailing_comma(expression: &Ex) -> bool {
    expression.is("trailing_comma")
}

pub fn is_expression_forcing_multiple_lines(
    expression: &Ex,
    standalone_comments: &[Option<String>],
) -> bool {
    if has_trailing_comma(expression) || is_multiline_string(expression) {
        return true;
    }
    if expression.is_token() {
        return false;
    }
    if has_standalone_comments(expression, standalone_comments)
        || is_multistatement_lambda(expression)
        || is_unistatement_lambda_with_compoud_statement(expression)
    {
        return true;
    }
    expression
        .children()
        .iter()
        .any(|child| is_expression_forcing_multiple_lines(child, standalone_comments))
}

pub fn expression_contains_lambda(expression: &Ex) -> bool {
    if expression.is_token() {
        return false;
    }
    if expression.data() == "lambda" {
        return true;
    }
    expression.children().iter().any(expression_contains_lambda)
}

pub fn is_any_comma(expression: &Ex) -> bool {
    expression.is("trailing_comma") || expression.kind() == Some("COMMA")
}

pub fn is_any_parentheses(expression: &Ex) -> bool {
    matches!(expression.kind(), Some("LPAR" | "RPAR"))
}

fn is_multiline_string(expression: &Ex) -> bool {
    expression.is("string")
        && expression.child(0).kind() == Some("LONG_STRING")
        && string::splitlines(expression.child(0).value()).len() > 1
}

fn has_standalone_comments(expression: &Ex, standalone_comments: &[Option<String>]) -> bool {
    let range = builtins::slice(
        standalone_comments.len(),
        Some(expression.line()),
        Some(expression.end_line()),
    );
    standalone_comments[range].iter().any(Option::is_some)
}

fn is_multistatement_lambda(expression: &Ex) -> bool {
    expression.is("lambda") && expression.len() > 2
}

// TODO: remove once such statements are supported
fn is_unistatement_lambda_with_compoud_statement(expression: &Ex) -> bool {
    if !(expression.is("lambda") && expression.len() == 2) {
        return false;
    }
    let statement = expression.child(1);
    !statement.is_token()
        && matches!(
            statement.data(),
            "if_stmt" | "while_stmt" | "for_stmt" | "for_stmt_typed" | "match_stmt"
        )
}
