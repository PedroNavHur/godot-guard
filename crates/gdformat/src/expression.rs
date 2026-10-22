use crate::constants::TAB_INDENT_SIZE;
use crate::context::{Context, ExpressionContext};
use crate::expression_to_str::expression_to_str;
use crate::expression_utils::{
    is_expression_forcing_multiple_lines, is_foldable, remove_outer_parentheses,
};
use crate::node::Ex;
use crate::py;
use crate::types::{FormattedLines, Outcome};

mod lists;
mod multiline;

use multiline::format_foldable_to_multiple_lines;
pub(crate) use multiline::format_operator_chain_based_expression_to_multiple_lines;

pub fn format_expression<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> Outcome {
    let concrete_expression = expression.child(0);
    (
        format_standalone_expression(&concrete_expression, expression_context, context),
        expression.end_line(),
    )
}

pub fn format_concrete_expression<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> Outcome {
    (
        format_concrete(expression, expression_context, context),
        expression.end_line(),
    )
}

pub(crate) fn format_standalone_expression<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let expression = remove_outer_parentheses(expression);
    format_concrete(&expression, expression_context, context)
}

/// `_format_concrete_expression`.
pub(crate) fn format_concrete<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    if is_foldable(expression) {
        return format_foldable(expression, expression_context, context);
    }
    format_concrete_expression_to_single_line(expression, expression_context, context)
}

fn format_concrete_expression_to_single_line(
    expression: &Ex,
    expression_context: &ExpressionContext,
    context: &Context,
) -> FormattedLines {
    vec![(
        Some(expression_context.prefix_line),
        format!(
            "{}{}{}{}",
            context.indent_string,
            expression_context.prefix_string,
            expression_to_str(expression),
            expression_context.suffix_string
        ),
    )]
}

/// The line's length with tabs counted as `TAB_INDENT_SIZE` columns.
pub(crate) fn tab_expanded_len(line: &str) -> i64 {
    py::len(line) + (TAB_INDENT_SIZE as i64 - 1) * line.matches('\t').count() as i64
}

fn format_foldable<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    if is_expression_forcing_multiple_lines(expression, context.standalone_comments) {
        return format_foldable_to_multiple_lines(expression, expression_context, context);
    }
    let single_line =
        format_concrete_expression_to_single_line(expression, expression_context, context);
    if tab_expanded_len(&single_line[0].1) <= context.max_line_length {
        return single_line;
    }
    format_foldable_to_multiple_lines(expression, expression_context, context)
}
