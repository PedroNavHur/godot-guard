//! The dot-chain, subscription, and lambda parts of gdtoolkit's `expression.py`.

use crate::block::format_block;
use crate::context::{Context, ExpressionContext};
use crate::expression::{
    format_concrete, format_operator_chain_based_expression_to_multiple_lines, tab_expanded_len,
};
use crate::expression_utils::{expression_contains_lambda, is_expression_forcing_multiple_lines};
use crate::function_statement::StatementFormatter;
use crate::node::Ex;
use crate::types::FormattedLines;
use pycompat::string;

pub(crate) fn format_subscription_to_multiple_lines<'a>(
    expression: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let subscriptee_expression_context = ExpressionContext::new(
        expression_context.prefix_string.clone(),
        expression_context.prefix_line,
        "",
        expression_context.suffix_line,
    );
    let mut subscriptee_lines = format_concrete(
        &expression.child(0),
        &subscriptee_expression_context,
        context,
    );
    let (last_line, last_text) = subscriptee_lines.pop().expect("no subscriptee lines");
    let last_line = last_line.expect("subscriptee line without a number");
    let subscript_expression_context = ExpressionContext::new(
        format!("{}[", string::strip(&last_text)),
        last_line,
        format!("]{}", expression_context.suffix_string),
        expression_context.suffix_line,
    );
    subscriptee_lines.extend(format_concrete(
        &expression.child(1),
        &subscript_expression_context,
        context,
    ));
    subscriptee_lines
}

pub(crate) fn collapse_getattr_tree_to_dot_chain<'a>(expression: &Ex<'a>) -> Ex<'a> {
    let mut reversed_dot_chain_children: Vec<Ex<'a>> = Vec::new();
    let mut pending_getattr_call_to_match: Option<Ex<'a>> = None;
    let mut next_expression_to_process = Some(expression.clone());
    while let Some(next) = next_expression_to_process.take() {
        if next.is_token() {
            reversed_dot_chain_children.push(next);
            continue;
        }
        match next.data() {
            "getattr_call" => {
                next_expression_to_process = Some(next.child(0));
                pending_getattr_call_to_match = Some(next);
            }
            "getattr" => {
                let children = next.children();
                match pending_getattr_call_to_match.take() {
                    None => reversed_dot_chain_children.extend(children[1..].iter().rev().cloned()),
                    Some(call) => {
                        let matching_attr = children[children.len() - 1].clone();
                        let mut fake_children = vec![matching_attr.clone()];
                        fake_children.extend(call.children_from(1));
                        reversed_dot_chain_children.push(Ex::fake(
                            "actual_getattr_call",
                            fake_children,
                            matching_attr.line(),
                            call.end_line(),
                        ));
                        let middle = &children[1..children.len() - 1];
                        reversed_dot_chain_children.extend(middle.iter().rev().cloned());
                    }
                }
                next_expression_to_process = Some(children[0].clone());
            }
            "subscr_expr" => {
                let sub_dot_chain = collapse_subscr_expr_tree_to_dot_chain(&next);
                reversed_dot_chain_children.extend(sub_dot_chain.children().into_iter().rev());
            }
            _ => reversed_dot_chain_children.push(next),
        }
    }
    reversed_dot_chain_children.reverse();
    let dot_chain_children = reversed_dot_chain_children;
    let line = dot_chain_children[0].line();
    let end_line = dot_chain_children[dot_chain_children.len() - 1].end_line();
    Ex::fake("dot_chain", dot_chain_children, line, end_line)
}

pub(crate) fn collapse_subscr_expr_tree_to_dot_chain<'a>(expression: &Ex<'a>) -> Ex<'a> {
    let subscriptee = expression.child(0);
    let subscript_to_match = expression.child(1);
    let mut sub_dot_chain = match subscriptee.is_token() {
        false if subscriptee.data() == "subscr_expr" => {
            collapse_subscr_expr_tree_to_dot_chain(&subscriptee).children()
        }
        false if matches!(subscriptee.data(), "getattr" | "getattr_call") => {
            collapse_getattr_tree_to_dot_chain(&subscriptee).children()
        }
        _ => vec![subscriptee],
    };
    let matching_expr = sub_dot_chain.pop().expect("empty dot chain");
    let line = matching_expr.line();
    sub_dot_chain.push(Ex::fake(
        "actual_subscr_expr",
        vec![matching_expr, subscript_to_match],
        line,
        expression.end_line(),
    ));
    let dot_chain_children = sub_dot_chain;
    let line = dot_chain_children[0].line();
    let end_line = dot_chain_children[dot_chain_children.len() - 1].end_line();
    Ex::fake("dot_chain", dot_chain_children, line, end_line)
}

pub(crate) fn format_dot_chain_to_multiple_lines<'a>(
    dot_chain: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    // temporary workaround for likely Godot bug
    // TODO: remove once not needed anymore (this probably should be kept until Godot 5 for compat)
    if expression_contains_lambda(dot_chain) {
        return format_dot_chain_to_multiple_lines_bottom_up(
            dot_chain,
            expression_context,
            context,
        );
    }
    if is_expression_forcing_multiple_lines(dot_chain, context.standalone_comments) {
        return format_operator_chain_based_expression_to_multiple_lines(
            dot_chain,
            expression_context,
            context,
        );
    }
    let lines_formatted_bottom_up =
        format_dot_chain_to_multiple_lines_bottom_up(dot_chain, expression_context, context);
    if lines_formatted_bottom_up
        .iter()
        .all(|(_, line)| tab_expanded_len(line) <= context.max_line_length)
    {
        return lines_formatted_bottom_up;
    }
    format_operator_chain_based_expression_to_multiple_lines(dot_chain, expression_context, context)
}

fn format_dot_chain_to_multiple_lines_bottom_up<'a>(
    dot_chain: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let mut children = dot_chain.children();
    let last_chain_element = children.pop().expect("empty dot chain");
    let foldable_last = !last_chain_element.is_token()
        && matches!(
            last_chain_element.data(),
            "actual_getattr_call" | "actual_subscr_expr"
        );
    if !foldable_last
        || (last_chain_element.data() == "actual_getattr_call"
            && children.iter().any(expression_contains_lambda))
    {
        return format_operator_chain_based_expression_to_multiple_lines(
            dot_chain,
            expression_context,
            context,
        );
    }
    let callee = last_chain_element.child(0);
    let callee_end_line = callee.end_line();
    children.push(callee);
    let new_dot_chain = Ex::fake(
        "non_foldable_dot_chain",
        children,
        dot_chain.line(),
        callee_end_line,
    );
    let mut new_children = vec![new_dot_chain.clone()];
    new_children.extend(last_chain_element.children_from(1));
    let new_actual_expr = Ex::fake(
        last_chain_element.data(),
        new_children,
        new_dot_chain.line(),
        last_chain_element.end_line(),
    );
    format_concrete(&new_actual_expr, expression_context, context)
}

pub(crate) fn format_lambda_to_multiple_lines<'a>(
    a_lambda: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let expression_context_for_header = ExpressionContext::new(
        expression_context.prefix_string.clone(),
        expression_context.prefix_line,
        "",
        -1,
    );
    let mut header_lines =
        format_concrete(&a_lambda.child(0), &expression_context_for_header, context);
    let mut child_context = context.create_child_context(expression_context.prefix_line);
    let (mut block_lines, _) = format_block(
        &a_lambda.children_from(1),
        StatementFormatter::Func,
        &mut child_context,
        crate::constants::default_surrounding_empty_lines,
    );
    let (last_block_line_number, last_block_line_content) =
        block_lines.pop().expect("lambda without block lines");
    block_lines.push((
        last_block_line_number,
        format!(
            "{last_block_line_content}{}",
            expression_context.suffix_string
        ),
    ));
    header_lines.extend(block_lines);
    header_lines
}

pub(crate) fn format_lambda_header_to_multiple_lines<'a>(
    lambda_header: &Ex<'a>,
    expression_context: &ExpressionContext,
    context: &Context<'a>,
) -> FormattedLines {
    let has_name = lambda_header.child(0).is_token();
    let append_to_prefix = match has_name {
        true => format!("func {}", lambda_header.child(0).value()),
        false => "func".to_owned(),
    };
    let args_offset = usize::from(has_name);
    let theres_something_after_args = lambda_header.len() > args_offset + 1;
    let optional_type_hint = match theres_something_after_args {
        true => format!(
            " -> {}",
            lambda_header.child(args_offset as isize + 1).value()
        ),
        false => String::new(),
    };
    let new_expression_context = ExpressionContext::new(
        format!("{}{append_to_prefix}", expression_context.prefix_string),
        expression_context.prefix_line,
        format!("{optional_type_hint}:{}", expression_context.suffix_string),
        expression_context.suffix_line,
    );
    format_concrete(
        &lambda_header.child(args_offset as isize),
        &new_expression_context,
        context,
    )
}
