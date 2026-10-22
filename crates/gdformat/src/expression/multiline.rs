//! `_format_foldable_to_multiple_lines` from gdtoolkit's `expression.py` and its handlers, except the list
//! and key/value ones (`lists`) and the dot-chain, subscription, and lambda ones (`crate::dot_chain`).

use super::lists::{
    format_args_to_multiple_lines, format_array_to_multiple_lines, format_comma_separated_list,
    format_comma_separated_list_to_multiple_lines,
    format_contextless_comma_separated_list_to_multiple_lines, format_dict_to_multiple_lines,
    format_kv_pair_to_multiple_lines,
};
use super::{
    format_concrete, format_concrete_expression_to_single_line, format_expression,
    format_standalone_expression,
};
use crate::context::{Context, ExpressionContext};
use crate::dot_chain::{
    collapse_getattr_tree_to_dot_chain, collapse_subscr_expr_tree_to_dot_chain,
    format_dot_chain_to_multiple_lines, format_lambda_header_to_multiple_lines,
    format_lambda_to_multiple_lines, format_subscription_to_multiple_lines,
};
use crate::expression_to_str::expression_to_str;
use crate::node::Ex;
use crate::py;
use crate::types::FormattedLines;

/// A `handlers` dict value: folds one kind of expression to multiple lines.
type FoldHandler = for<'a> fn(&Ex<'a>, &ExpressionContext, &Context<'a>) -> FormattedLines;

/// The names the Python's `handlers` dict maps to `_format_operator_chain_based_expression_to_multiple_lines`.
const OPERATOR_CHAIN_EXPRESSIONS: &[&str] = &[
    "test_expr",
    "asless_test_expr",
    "or_test",
    "asless_or_test",
    "and_test",
    "asless_and_test",
    "content_test",
    "asless_content_test",
    "comparison",
    "asless_comparison",
    "bitw_or",
    "asless_bitw_or",
    "bitw_xor",
    "asless_bitw_xor",
    "bitw_and",
    "asless_bitw_and",
    "shift_expr",
    "asless_shift_expr",
    "arith_expr",
    "asless_arith_expr",
    "mdr_expr",
    "asless_mdr_expr",
    "pow_expr",
    "asless_pow_expr",
    "type_test",
    "asless_type_test",
    "actual_type_cast",
    "test_pattern",
    "or_pattern",
    "and_pattern",
    "comp_pattern",
    "bitw_or_pattern",
    "bitw_xor_pattern",
    "bitw_and_pattern",
    "shift_pattern",
    "arith_pattern",
    "mdr_pattern",
    "attr_pattern",
];

/// The rest of gdtoolkit's `handlers` dict of `_format_foldable_to_multiple_lines`.
const HANDLERS: &[(&str, FoldHandler)] = &[
    (
        "assnmnt_expr",
        format_assignment_expression_to_multiple_lines,
    ),
    ("asless_actual_not_test", format_not_test_to_multiple_lines),
    ("asless_actual_neg_expr", format_unary_to_multiple_lines),
    ("asless_actual_bitw_not", format_unary_to_multiple_lines),
    ("await_expr", format_await_expression_to_multiple_lines),
    ("standalone_call", format_call_expression_to_multiple_lines),
    (
        "actual_getattr_call",
        format_call_expression_to_multiple_lines,
    ),
    ("call_pattern", format_call_expression_to_multiple_lines),
    ("getattr_call", format_getattr_to_multiple_lines),
    ("getattr", format_getattr_to_multiple_lines),
    ("subscr_expr", format_subscr_expr_to_multiple_lines),
    ("par_expr", format_parentheses_to_multiple_lines),
    ("par_pattern", format_parentheses_to_multiple_lines),
    ("array", format_array_to_multiple_lines),
    ("array_pattern", format_array_to_multiple_lines),
    ("string", format_string_to_multiple_lines),
    ("dict", format_dict_to_multiple_lines),
    ("enum_body", format_dict_to_multiple_lines),
    ("dict_pattern", format_dict_to_multiple_lines),
    ("c_dict_element", format_kv_pair_to_multiple_lines),
    ("eq_dict_element", format_kv_pair_to_multiple_lines),
    ("kv_pair_pattern", format_kv_pair_to_multiple_lines),
    ("lambda", format_lambda_to_multiple_lines),
    ("lambda_header", format_lambda_header_to_multiple_lines),
    // fake expressions:
    ("func_args", format_args_to_multiple_lines),
    ("signal_args", format_args_to_multiple_lines),
    ("annotation_args", format_args_to_multiple_lines),
    (
        "func_arg_variadic",
        format_func_arg_variadic_to_multiple_lines,
    ),
    ("func_arg_regular", format_func_arg_to_multiple_lines),
    ("func_arg_inf", format_func_arg_to_multiple_lines),
    ("func_arg_typed", format_func_arg_to_multiple_lines),
    (
        "comma_separated_list",
        format_comma_separated_list_to_multiple_lines,
    ),
    ("contextless_comma_separated_list", |e, _, c| {
        format_contextless_comma_separated_list_to_multiple_lines(e, c)
    }),
    ("contextless_operator_chain_based_expression", |e, _, c| {
        format_contextless_operator_chain_based_expression_to_multiple_lines(e, c)
    }),
    ("annotation", |e, _, c| {
        format_annotation_to_multiple_lines(e, c)
    }),
    ("dot_chain", format_dot_chain_to_multiple_lines),
    ("actual_subscr_expr", format_subscription_to_multiple_lines),
    // patterns (fake expressions):
    ("list_pattern", format_concrete_expression_to_single_line),
    ("not_pattern", format_concrete_expression_to_single_line),
    ("neg_pattern", format_concrete_expression_to_single_line),
    (
        "bitw_not_pattern",
        format_concrete_expression_to_single_line,
    ),
];

fn fold_handler(data: &str) -> Option<FoldHandler> {
    if OPERATOR_CHAIN_EXPRESSIONS.contains(&data) {
        return Some(format_operator_chain_based_expression_to_multiple_lines);
    }
    HANDLERS
        .iter()
        .find(|(name, _)| *name == data)
        .map(|(_, handler)| *handler)
}

pub(super) fn format_foldable_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let Some(handler) = fold_handler(expression.data()) else {
        panic!(
            "format_foldable_to_multiple_lines: no handler for {}",
            expression.data()
        );
    };
    handler(expression, expression_context, context)
}

fn format_unary_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let operator = expression_to_str(&expression.child(0));
    append_to_expression_context_and_pass(
        &operator,
        &expression.child(1),
        expression_context,
        context,
    )
}

fn format_getattr_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let dot_chain = collapse_getattr_tree_to_dot_chain(expression);
    format_foldable_to_multiple_lines(&dot_chain, expression_context, context)
}

fn format_subscr_expr_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let mut dot_chain = collapse_subscr_expr_tree_to_dot_chain(expression);
    if dot_chain.len() == 1 {
        dot_chain = dot_chain.child(0);
    }
    format_foldable_to_multiple_lines(&dot_chain, expression_context, context)
}

fn format_func_arg_variadic_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    append_to_expression_context_and_pass("...", &expression.child(0), expression_context, context)
}

fn format_parentheses_to_multiple_lines<'a>(
    par_expr: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let new_expression_context = ExpressionContext::new(
        format!("{}(", expression_context.prefix_string),
        expression_context.prefix_line,
        format!("){}", expression_context.suffix_string),
        par_expr.end_line(),
    );
    format_standalone_expression(&par_expr.child(0), &new_expression_context, context)
}

fn format_string_to_multiple_lines(
    string: &Ex,
    expression_context: &ExpressionContext,
    context: &Context,
) -> FormattedLines {
    let long_string = string.child(0);
    let lines = py::splitlines(long_string.value());
    let mut formatted_lines = vec![(
        Some(expression_context.prefix_line),
        format!(
            "{}{}{}",
            context.indent_string, expression_context.prefix_string, lines[0]
        ),
    )];
    for middle_line in &lines[1..lines.len() - 1] {
        formatted_lines.push((Some(string.line()), (*middle_line).to_owned()));
    }
    formatted_lines.push((
        Some(string.line()),
        format!(
            "{}{}",
            lines[lines.len() - 1],
            expression_context.suffix_string
        ),
    ));
    formatted_lines
}

fn format_not_test_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let spacing = if expression.child(0).value() == "!" {
        ""
    } else {
        " "
    };
    append_to_expression_context_and_pass(
        &format!("{}{spacing}", expression_to_str(&expression.child(0))),
        &expression.child(1),
        expression_context,
        context,
    )
}

fn format_assignment_expression_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let new_expression_context = ExpressionContext::new(
        format!(
            "{}{} {} ",
            expression_context.prefix_string,
            expression_to_str(&expression.child(0)),
            expression_to_str(&expression.child(1))
        ),
        expression_context.prefix_line,
        expression_context.suffix_string.clone(),
        expression_context.suffix_line,
    );
    format_concrete(&expression.child(2), &new_expression_context, context)
}

fn format_func_arg_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let data = expression.data();
    if data == "func_arg_regular" && expression.len() == 1 {
        return format_concrete(&expression.child(0), expression_context, context);
    }
    if data == "func_arg_typed" && expression.len() == 2 {
        return append_to_expression_context_and_pass(
            &format!("{}: ", expression.child(0).value()),
            &expression.child(1),
            expression_context,
            context,
        );
    }
    let name = expression.child(0).value();
    let prefix = match data {
        "func_arg_regular" => format!("{name} = "),
        "func_arg_inf" => format!("{name} := "),
        _ => format!("{name}: {} = ", expression.child(1).value()),
    };
    let new_expression_context = ExpressionContext::new(
        prefix,
        expression_context.prefix_line,
        expression_context.suffix_string.clone(),
        expression_context.suffix_line,
    );
    format_expression(&expression.child(-1), &new_expression_context, context).0
}

fn format_call_expression_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let callee_node = expression.child(0);
    let callee = expression_to_str(&callee_node);
    if expression.len() == 1 {
        return vec![(
            Some(expression_context.prefix_line),
            format!(
                "{}{}{callee}(){}",
                context.indent_string,
                expression_context.prefix_string,
                expression_context.suffix_string
            ),
        )];
    }
    let new_expression_context = ExpressionContext::new(
        format!("{}{callee}(", expression_context.prefix_string),
        callee_node.line(),
        format!("){}", expression_context.suffix_string),
        expression.end_line(),
    );
    format_comma_separated_list(
        expression.children_from(1),
        &new_expression_context,
        context,
    )
}

pub(crate) fn format_operator_chain_based_expression_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let (prefix, suffix) = (
        &expression_context.prefix_string,
        &expression_context.suffix_string,
    );
    let inside_par = (prefix.ends_with('(') && suffix.starts_with(')'))
        || (prefix.ends_with('[') && suffix.starts_with(']'));
    let (lpar, rpar) = if inside_par { ("", "") } else { ("(", ")") };
    let child_context = context.create_child_context(expression_context.prefix_line);
    let child_expression_context = ExpressionContext::new(
        "",
        expression_context.prefix_line,
        "",
        expression_context.suffix_line,
    );
    let fake_expression = Ex::fake(
        "contextless_operator_chain_based_expression",
        expression.children(),
        expression_context.prefix_line,
        expression_context.suffix_line,
    );
    let mut formatted_lines = vec![(
        Some(expression_context.prefix_line),
        format!("{}{prefix}{lpar}", context.indent_string),
    )];
    formatted_lines.extend(format_concrete(
        &fake_expression,
        &child_expression_context,
        &child_context,
    ));
    formatted_lines.push((
        Some(expression.child(-1).end_line()),
        format!("{}{rpar}{suffix}", context.indent_string),
    ));
    formatted_lines
}

fn format_contextless_operator_chain_based_expression_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    context: &Context<'a>,
) -> FormattedLines {
    let children = expression.children();
    let value = &children[0];
    let mut formatted_lines = format_concrete(
        value,
        &ExpressionContext::new("", value.line(), "", value.end_line()),
        context,
    );
    for [operator, child] in children[1..].as_chunks::<2>().0 {
        formatted_lines.extend(format_concrete(
            child,
            &ExpressionContext::new(
                format!("{} ", expression_to_str(operator)),
                child.line(),
                "",
                child.end_line(),
            ),
            context,
        ));
    }
    formatted_lines
}

fn append_to_expression_context_and_pass<'a>(
    str_to_append: &str,
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let mut new_expression_context = expression_context.clone();
    new_expression_context.prefix_string.push_str(str_to_append);
    format_concrete(expression, &new_expression_context, context)
}

fn format_await_expression_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let children = expression.children();
    let (operand, awaits) = children.split_last().expect("empty await_expr");
    let awaits: Vec<&str> = awaits.iter().map(Ex::value).collect();
    append_to_expression_context_and_pass(
        &format!("{} ", awaits.join(" ")),
        operand,
        expression_context,
        context,
    )
}

fn format_annotation_to_multiple_lines<'a>(
    annotation: &Ex<'a>,
    context: &Context<'a>,
) -> FormattedLines {
    let annotation_name = annotation.child(0).value();
    if annotation.len() == 1 {
        return vec![(
            Some(annotation.line()),
            format!("{}@{annotation_name}", context.indent_string),
        )];
    }
    let new_expression_context =
        ExpressionContext::new(format!("@{annotation_name}"), annotation.line(), "", -1);
    format_concrete(&annotation.child(-1), &new_expression_context, context)
}
