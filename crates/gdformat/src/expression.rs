use crate::constants::TAB_INDENT_SIZE;
use crate::context::{Context, ExpressionContext};
use crate::dot_chain::{
    collapse_getattr_tree_to_dot_chain, collapse_subscr_expr_tree_to_dot_chain,
    format_dot_chain_to_multiple_lines, format_lambda_header_to_multiple_lines,
    format_lambda_to_multiple_lines, format_subscription_to_multiple_lines,
};
use crate::expression_to_str::expression_to_str;
use crate::expression_utils::{
    is_any_comma, is_expression_forcing_multiple_lines, is_foldable, is_trailing_comma,
    remove_outer_parentheses,
};
use crate::node::Ex;
use crate::py;
use crate::types::{FormattedLines, Outcome};

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

fn format_comma_separated_list<'a>(
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

pub(crate) fn format_foldable_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let (e, ec, c) = (expression, expression_context, context);
    match e.data() {
        "assnmnt_expr" => format_assignment_expression_to_multiple_lines(e, ec, c),
        "test_expr"
        | "asless_test_expr"
        | "or_test"
        | "asless_or_test"
        | "and_test"
        | "asless_and_test"
        | "content_test"
        | "asless_content_test"
        | "comparison"
        | "asless_comparison"
        | "bitw_or"
        | "asless_bitw_or"
        | "bitw_xor"
        | "asless_bitw_xor"
        | "bitw_and"
        | "asless_bitw_and"
        | "shift_expr"
        | "asless_shift_expr"
        | "arith_expr"
        | "asless_arith_expr"
        | "mdr_expr"
        | "asless_mdr_expr"
        | "pow_expr"
        | "asless_pow_expr"
        | "type_test"
        | "asless_type_test"
        | "actual_type_cast"
        | "test_pattern"
        | "or_pattern"
        | "and_pattern"
        | "comp_pattern"
        | "bitw_or_pattern"
        | "bitw_xor_pattern"
        | "bitw_and_pattern"
        | "shift_pattern"
        | "arith_pattern"
        | "mdr_pattern"
        | "attr_pattern" => format_operator_chain_based_expression_to_multiple_lines(e, ec, c),
        "asless_actual_not_test" => format_not_test_to_multiple_lines(e, ec, c),
        "asless_actual_neg_expr" | "asless_actual_bitw_not" => {
            let operator = expression_to_str(&e.child(0));
            append_to_expression_context_and_pass(&operator, &e.child(1), ec, c)
        }
        "await_expr" => format_await_expression_to_multiple_lines(e, ec, c),
        "standalone_call" | "actual_getattr_call" | "call_pattern" => {
            format_call_expression_to_multiple_lines(e, ec, c)
        }
        "getattr_call" | "getattr" => {
            format_foldable_to_multiple_lines(&collapse_getattr_tree_to_dot_chain(e), ec, c)
        }
        "subscr_expr" => {
            let mut dot_chain = collapse_subscr_expr_tree_to_dot_chain(e);
            if dot_chain.len() == 1 {
                dot_chain = dot_chain.child(0);
            }
            format_foldable_to_multiple_lines(&dot_chain, ec, c)
        }
        "par_expr" | "par_pattern" => format_parentheses_to_multiple_lines(e, ec, c),
        "array" | "array_pattern" => format_enclosed(e, "[", "]", ec, c),
        "string" => format_string_to_multiple_lines(e, ec, c),
        "dict" | "enum_body" | "dict_pattern" => format_enclosed(e, "{", "}", ec, c),
        "c_dict_element" | "eq_dict_element" | "kv_pair_pattern" => {
            format_kv_pair_to_multiple_lines(e, ec, c)
        }
        "lambda" => format_lambda_to_multiple_lines(e, ec, c),
        "lambda_header" => format_lambda_header_to_multiple_lines(e, ec, c),
        // fake expressions:
        "func_args" | "signal_args" | "annotation_args" => format_enclosed(e, "(", ")", ec, c),
        "func_arg_variadic" => append_to_expression_context_and_pass("...", &e.child(0), ec, c),
        "func_arg_regular" | "func_arg_inf" | "func_arg_typed" => {
            format_func_arg_to_multiple_lines(e, ec, c)
        }
        "comma_separated_list" => format_comma_separated_list_to_multiple_lines(e, ec, c),
        "contextless_comma_separated_list" => {
            format_contextless_comma_separated_list_to_multiple_lines(e, c)
        }
        "contextless_operator_chain_based_expression" => {
            format_contextless_operator_chain_based_expression_to_multiple_lines(e, c)
        }
        "annotation" => format_annotation_to_multiple_lines(e, c),
        "dot_chain" => format_dot_chain_to_multiple_lines(e, ec, c),
        "actual_subscr_expr" => format_subscription_to_multiple_lines(e, ec, c),
        // patterns (fake expressions):
        "list_pattern" | "not_pattern" | "neg_pattern" | "bitw_not_pattern" => {
            format_concrete_expression_to_single_line(e, ec, c)
        }
        other => panic!("format_foldable_to_multiple_lines: no handler for {other}"),
    }
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

fn format_kv_pair_to_multiple_lines<'a>(
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

fn format_comma_separated_list_to_multiple_lines<'a>(
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

fn format_contextless_comma_separated_list_to_multiple_lines<'a>(
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

pub(crate) fn append_to_expression_context_and_pass<'a>(
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
