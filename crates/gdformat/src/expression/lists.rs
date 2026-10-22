//! The comma-separated-list and key/value parts of gdtoolkit's `expression.py`.

use super::{format_concrete, format_standalone_expression};
use crate::context::{Context, ExpressionContext};
use crate::expression_utils::{is_any_comma, is_trailing_comma};
use crate::node::Ex;
use crate::types::FormattedLines;

pub(super) fn format_array_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    format_enclosed(expression, "[", "]", expression_context, context)
}

pub(super) fn format_dict_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    format_enclosed(expression, "{", "}", expression_context, context)
}

pub(super) fn format_args_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    format_enclosed(expression, "(", ")", expression_context, context)
}

pub(super) fn format_comma_separated_list<'a>(
    a_list: Vec<Ex<'a>>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let fake_expression = Ex::fake(
        "comma_separated_list",
        a_list,
        expression_context.prefix_line,
        expression_context.suffix_line,
    );
    format_concrete(&fake_expression, expression_context, context)
}

/// `_format_array_to_multiple_lines`, `_format_dict_to_multiple_lines` and
/// `_format_args_to_multiple_lines`: the children between `open` and `close`, one per line.
fn format_enclosed<'a>(
    expression: &Ex<'a>,
    open: &str,
    close: &str,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let new_expression_context = ExpressionContext::new(
        format!("{}{open}", expression_context.prefix_string),
        expression_context.prefix_line,
        format!("{close}{}", expression_context.suffix_string),
        expression.end_line(),
    );
    format_comma_separated_list(expression.children(), &new_expression_context, context)
}

pub(super) fn format_kv_pair_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let suffix = match expression.data() {
        "c_dict_element" | "kv_pair_pattern" => ":",
        _ => " =",
    };
    let key_expression_context = ExpressionContext::new(
        expression_context.prefix_string.clone(),
        expression_context.prefix_line,
        suffix,
        expression_context.suffix_line,
    );
    let mut key_lines =
        format_standalone_expression(&expression.child(0), &key_expression_context, context);
    let value_expression_context = ExpressionContext::new(
        "",
        -1,
        expression_context.suffix_string.clone(),
        expression_context.suffix_line,
    );
    key_lines.extend(format_standalone_expression(
        &expression.child(1),
        &value_expression_context,
        context,
    ));
    key_lines
}

pub(super) fn format_comma_separated_list_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let a_list = expression.children();
    let child_context = context.create_child_context(expression_context.prefix_line);
    let child_expression_context = ExpressionContext::new(
        "",
        expression_context.prefix_line,
        "",
        expression_context.suffix_line,
    );
    let list_is_empty = a_list.is_empty();
    let fake_expression = Ex::fake(
        "contextless_comma_separated_list",
        a_list,
        expression_context.prefix_line,
        expression_context.suffix_line,
    );
    let mut formatted_lines = vec![(
        Some(expression_context.prefix_line),
        format!(
            "{}{}",
            context.indent_string, expression_context.prefix_string
        ),
    )];
    if !list_is_empty {
        formatted_lines.extend(format_concrete(
            &fake_expression,
            &child_expression_context,
            &child_context,
        ));
    }
    formatted_lines.push((
        Some(expression_context.suffix_line),
        format!(
            "{}{}",
            context.indent_string, expression_context.suffix_string
        ),
    ));
    formatted_lines
}

pub(super) fn format_contextless_comma_separated_list_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    context: &Context<'a>,
) -> FormattedLines {
    let a_list = expression.children();
    let elements: Vec<&Ex> = a_list.iter().filter(|node| !is_any_comma(node)).collect();
    let trailing_comma_present = a_list.last().is_some_and(is_trailing_comma);
    let mut formatted_lines = FormattedLines::new();
    for (i, element) in elements.iter().enumerate() {
        let suffix = if i != elements.len() - 1 || trailing_comma_present {
            ","
        } else {
            ""
        };
        let child_expression_context =
            ExpressionContext::new("", element.line(), suffix, element.end_line());
        formatted_lines.extend(format_standalone_expression(
            element,
            &child_expression_context,
            context,
        ));
    }
    formatted_lines
}
