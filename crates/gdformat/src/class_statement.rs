use crate::annotation::format_standalone_annotation;
use crate::block::format_block;
use crate::const_statement::format_const_statement;
use crate::constants::default_surrounding_empty_lines;
use crate::context::{Context, ExpressionContext};
use crate::expression::format_concrete_expression;
use crate::expression_to_str::expression_to_str;
use crate::function_statement::StatementFormatter;
use crate::node::Ex;
use crate::property::{
    append_property_body_to_formatted_line, format_property_body, has_inline_property_body,
};
use crate::statement_utils::format_simple_statement;
use crate::types::Outcome;
use crate::var_statement::format_var_statement;

pub fn format_class_statement<'a>(statement: &Ex<'a>, context: &mut Context<'a>) -> Outcome {
    match statement.data() {
        "pass_stmt" => format_simple_statement("pass", statement, context),
        "enum_stmt" => format_enum_statement(statement, context),
        "signal_stmt" => format_signal_statement(statement, context),
        "extends_stmt" => format_extends_statement(statement, context),
        "classname_stmt" => {
            let line = format!(
                "{}class_name {}",
                context.indent_string,
                statement.child(0).value()
            );
            (vec![(Some(statement.line()), line)], statement.line())
        }
        "classname_extends_stmt" => format_classname_extends_statement(statement, context),
        "class_var_stmt" => format_class_var_statement(statement, context, ""),
        "static_class_var_stmt" => {
            format_class_var_statement(&statement.child(0), context, "static ")
        }
        "const_stmt" => format_const_statement(statement, context),
        "docstr_stmt" => {
            let expression_context =
                ExpressionContext::new("", statement.line(), "", statement.end_line());
            format_concrete_expression(&statement.child(0), &expression_context, context)
        }
        "class_def" => format_class_def(statement, context),
        "func_def" => format_func_def(statement, context, ""),
        "static_func_def" => format_func_def(&statement.child(0), context, "static "),
        "abstract_func_def" => format_abstract_func_header(&statement.child(0), context),
        "annotation" => format_standalone_annotation(statement, context),
        "property_body_def" => format_property_body(statement, context),
        other => panic!("format_class_statement: no handler for {other}"),
    }
}

fn format_signal_statement<'a>(statement: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let name = statement.child(0).value();
    if statement.len() == 1 || statement.child(1).is_empty() {
        return format_simple_statement(&format!("signal {name}"), statement, context);
    }
    let expression_context = ExpressionContext::new(
        format!("signal {name}"),
        statement.line(),
        "",
        statement.end_line(),
    );
    format_concrete_expression(&statement.child(-1), &expression_context, context)
}

/// `.attr.attr` for the children from `start` on, or nothing.
fn optional_attributes(statement: &Ex, start: usize) -> String {
    let attributes: Vec<String> = statement
        .children_from(start)
        .iter()
        .map(expression_to_str)
        .collect();
    match attributes.is_empty() {
        true => String::new(),
        false => format!(".{}", attributes.join(".")),
    }
}

fn format_extends_statement(statement: &Ex, context: &Context) -> Outcome {
    let line = format!(
        "{}extends {}{}",
        context.indent_string,
        expression_to_str(&statement.child(0)),
        optional_attributes(statement, 1)
    );
    (vec![(Some(statement.line()), line)], statement.line())
}

fn format_classname_extends_statement(statement: &Ex, context: &Context) -> Outcome {
    let extendee_pos = 2 + 1;
    let line = format!(
        "{}class_name {} extends {}{}",
        context.indent_string,
        statement.child(1).value(),
        expression_to_str(&statement.child(extendee_pos)),
        optional_attributes(statement, extendee_pos as usize + 1)
    );
    (vec![(Some(statement.line()), line)], statement.line())
}

fn format_class_var_statement<'a>(
    statement: &Ex<'a>,
    context: &Context<'a>,
    prefix: &str,
) -> Outcome {
    let (mut formatted_lines, last_processed_line) =
        format_var_statement(statement, context, prefix);
    let concrete_var_statement = statement.child(0);
    if has_inline_property_body(&concrete_var_statement) {
        let inline_property_body = concrete_var_statement.child(-1);
        let last = formatted_lines.pop().expect("var statement without lines");
        formatted_lines.extend(append_property_body_to_formatted_line(
            last,
            &inline_property_body,
            context,
        ));
    }
    (formatted_lines, last_processed_line)
}

fn format_class_def<'a>(statement: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let name = statement.child(0).value();
    let mut formatted_lines = vec![(
        Some(statement.line()),
        format!("{}class {name}:", context.indent_string),
    )];
    let (class_lines, last_processed_line_no) = format_block(
        &statement.children_from(1),
        StatementFormatter::Class,
        &mut context.create_child_context(statement.line()),
        default_surrounding_empty_lines,
    );
    formatted_lines.extend(class_lines);
    (formatted_lines, last_processed_line_no)
}

fn format_func_def<'a>(statement: &Ex<'a>, context: &Context<'a>, prefix: &str) -> Outcome {
    let func_header = statement.child(0);
    let name = func_header.child(0).value();
    let has_return_type = func_header.len() > 2;
    let suffix = match has_return_type {
        true => format!(" -> {}:", func_header.child(2).value()),
        false => ":".to_owned(),
    };
    let expression_context = ExpressionContext::new(
        format!("{prefix}func {name}"),
        func_header.line(),
        suffix,
        func_header.end_line(),
    );
    let (mut formatted_lines, last_processed_line_no) =
        format_concrete_expression(&func_header.child(1), &expression_context, context);
    let (func_lines, last_processed_line_no) = format_block(
        &statement.children_from(1),
        StatementFormatter::Func,
        &mut context.create_child_context(last_processed_line_no),
        default_surrounding_empty_lines,
    );
    formatted_lines.extend(func_lines);
    (formatted_lines, last_processed_line_no)
}

fn format_enum_statement<'a>(statement: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let actual_enum = statement.child(0);
    let prefix = match actual_enum.len() == 2 {
        true => format!("enum {} ", actual_enum.child(0).value()),
        false => "enum ".to_owned(),
    };
    let expression_context =
        ExpressionContext::new(prefix, statement.line(), "", statement.end_line());
    format_concrete_expression(&actual_enum.child(-1), &expression_context, context)
}

fn format_abstract_func_header<'a>(statement: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let name = statement.child(0).value();
    let suffix = match statement.len() > 2 {
        true => format!(" -> {}", statement.child(2).value()),
        false => String::new(),
    };
    let expression_context = ExpressionContext::new(
        format!("func {name}"),
        statement.line(),
        suffix,
        statement.end_line(),
    );
    format_concrete_expression(&statement.child(1), &expression_context, context)
}
