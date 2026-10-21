use crate::annotation::format_standalone_annotation;
use crate::block::{format_block, reconstruct_blank_lines_in_range};
use crate::class_statement::format_class_statement;
use crate::const_statement::format_const_statement;
use crate::constants::default_surrounding_empty_lines;
use crate::context::{Context, ExpressionContext};
use crate::expression::format_expression;
use crate::expression_to_str::expression_to_str;
use crate::node::Ex;
use crate::property::format_property_statement;
use crate::statement_utils::format_simple_statement;
use crate::types::{FormattedLines, Outcome};
use crate::var_statement::format_var_statement;

/// The `statement_formatter` `format_block` is given; the Python passes the function itself and tests
/// its `__name__`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatementFormatter {
    Class,
    Func,
    Property,
}

impl StatementFormatter {
    pub fn format<'a>(self, statement: &Ex<'a>, context: &mut Context<'a>) -> Outcome {
        match self {
            StatementFormatter::Class => format_class_statement(statement, context),
            StatementFormatter::Func => format_func_statement(statement, context),
            StatementFormatter::Property => format_property_statement(statement, context),
        }
    }
}

pub fn format_func_statement<'a>(statement: &Ex<'a>, context: &mut Context<'a>) -> Outcome {
    match statement.data() {
        "pass_stmt" => format_simple_statement("pass", statement, context),
        "func_var_stmt" => format_var_statement(statement, context, ""),
        "const_stmt" => format_const_statement(statement, context),
        "expr_stmt" => format_expr_statement(statement, context),
        "return_stmt" => format_return_statement(statement, context),
        "break_stmt" => format_simple_statement("break", statement, context),
        "breakpoint_stmt" => format_simple_statement("breakpoint", statement, context),
        "continue_stmt" => format_simple_statement("continue", statement, context),
        "if_stmt" => format_if_statement(statement, context),
        "while_stmt" => format_branch("while ", ":", Some(0), statement, context),
        "for_stmt" => {
            let prefix = format!("for {} in ", statement.child(0).value());
            format_branch(&prefix, ":", Some(1), statement, context)
        }
        "for_stmt_typed" => {
            let prefix = format!(
                "for {}: {} in ",
                statement.child(0).value(),
                statement.child(1).value()
            );
            format_branch(&prefix, ":", Some(2), statement, context)
        }
        "match_stmt" => format_branch("match ", ":", Some(0), statement, context),
        "annotation" => format_standalone_annotation(statement, context),
        // fake statements:
        "match_branch" => format_branch("", ":", Some(0), statement, context),
        "guarded_match_branch" => {
            // TODO: fold pattern as well
            let pattern_str = expression_to_str(&statement.child(0).child(0));
            format_branch(
                &format!("{pattern_str} when "),
                ":",
                Some(1),
                statement,
                context,
            )
        }
        other => panic!("format_func_statement: no handler for {other}"),
    }
}

fn format_expr_statement<'a>(statement: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let expression_context = ExpressionContext::new("", statement.line(), "", statement.line());
    format_expression(&statement.child(0), &expression_context, context)
}

fn format_return_statement<'a>(statement: &Ex<'a>, context: &Context<'a>) -> Outcome {
    if statement.is_empty() {
        return format_simple_statement("return", statement, context);
    }
    let expression_context =
        ExpressionContext::new("return ", statement.line(), "", statement.line());
    format_expression(&statement.child(0), &expression_context, context)
}

fn format_if_statement<'a>(statement: &Ex<'a>, context: &mut Context<'a>) -> Outcome {
    let mut formatted_lines = FormattedLines::new();
    let mut previously_processed_line_number = None;
    for branch in statement.children() {
        if let Some(previous) = previously_processed_line_number {
            formatted_lines.extend(reconstruct_blank_lines_in_range(
                previous,
                branch.line(),
                context,
            ));
        }
        let (prefix, expr_position) = match branch.data() {
            "if_branch" => ("if ", Some(0)),
            "elif_branch" => ("elif ", Some(0)),
            "else_branch" => ("else", None),
            other => panic!("format_if_statement: unexpected branch {other}"),
        };
        let (lines, last_line) = format_branch(prefix, ":", expr_position, &branch, context);
        previously_processed_line_number = Some(last_line);
        formatted_lines.extend(lines);
    }
    let last_line = previously_processed_line_number.expect("if statement without branches");
    (formatted_lines, last_line)
}

fn format_branch<'a>(
    prefix: &str,
    suffix: &str,
    expr_position: Option<usize>,
    statement: &Ex<'a>,
    context: &Context<'a>,
) -> Outcome {
    let (mut header_lines, last_processed_line_no, offset) = match expr_position {
        Some(position) => {
            let expression_context =
                ExpressionContext::new(prefix, statement.line(), suffix, statement.line());
            let (lines, last_line) = format_expression(
                &statement.child(position as isize),
                &expression_context,
                context,
            );
            (lines, last_line, position + 1)
        }
        None => (
            vec![(
                Some(statement.line()),
                format!("{}{prefix}{suffix}", context.indent_string),
            )],
            statement.line(),
            0,
        ),
    };
    let (body_lines, last_processed_line_no) = format_block(
        &statement.children_from(offset),
        StatementFormatter::Func,
        &mut context.create_child_context(last_processed_line_no),
        default_surrounding_empty_lines,
    );
    header_lines.extend(body_lines);
    (header_lines, last_processed_line_no)
}
