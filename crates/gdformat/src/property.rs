use std::rc::Rc;

use crate::block::format_block;
use crate::constants::default_surrounding_empty_lines;
use crate::context::Context;
use crate::expression_to_str::expression_to_str;
use crate::function_statement::StatementFormatter;
use crate::node::{Ex, Fake};
use crate::types::{FormattedLine, FormattedLines, Outcome};

pub fn has_inline_property_body(statement: &Ex) -> bool {
    statement
        .children()
        .iter()
        .any(|child| child.is("inline_property_body"))
}

pub fn append_property_body_to_formatted_line<'a>(
    line_to_append_to: FormattedLine,
    inline_property_body: &Ex<'a>,
    context: &Context<'a>,
) -> FormattedLines {
    let mut formatted_lines = vec![(line_to_append_to.0, format!("{}:", line_to_append_to.1))];
    let property_delegates = inline_property_body.children();
    if !property_delegates.is_empty() {
        let line = property_delegates[0].line();
        let fake_property_body = Ex::Fake(Rc::new(Fake {
            data: "property_body",
            children: property_delegates,
            line,
            end_line: None,
        }));
        formatted_lines.extend(format_property_body(&fake_property_body, context).0);
    }
    formatted_lines
}

pub fn format_property_body<'a>(property_body: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let (mut formatted_lines, last_processed_line) = format_block(
        &property_body.children(),
        StatementFormatter::Property,
        &mut context.create_child_context(property_body.line()),
        default_surrounding_empty_lines,
    );
    let first = property_body.child(0);
    if first.data().starts_with("property_delegate") && property_body.len() > 1 {
        let first_line = Some(first.line());
        if let Some(line) = formatted_lines
            .iter_mut()
            .find(|(number, _)| *number == first_line)
        {
            line.1.push(',');
        }
    }
    (formatted_lines, last_processed_line)
}

pub fn format_property_statement<'a>(statement: &Ex<'a>, context: &mut Context<'a>) -> Outcome {
    match statement.data() {
        "property_delegate_set" | "property_delegate_get" => {
            let line = format!(
                "{}{} = {}",
                context.indent_string,
                statement.child(0).value(),
                statement.child(2).value()
            );
            (vec![(Some(statement.line()), line)], statement.line())
        }
        "property_custom_getter" => format_property_getter(statement, context),
        "property_custom_setter" => format_property_setter(statement, context),
        other => panic!("format_property_statement: no handler for {other}"),
    }
}

fn format_property_setter<'a>(property_setter: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let argument_name = property_setter.child(1).value();
    let header = format!("{}set({argument_name}):", context.indent_string);
    format_accessor(header, property_setter, 2, context)
}

fn format_property_getter<'a>(property_getter: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let has_args = property_getter.child(1).is("property_custom_getter_args");
    let args = match has_args {
        true => expression_to_str(&property_getter.child(1)),
        false => String::new(),
    };
    let header = format!("{}get{args}:", context.indent_string);
    format_accessor(
        header,
        property_getter,
        if has_args { 2 } else { 1 },
        context,
    )
}

/// A custom getter's or setter's header line, then its statements as a function body.
fn format_accessor<'a>(
    header: String,
    accessor: &Ex<'a>,
    statements_offset: usize,
    context: &Context<'a>,
) -> Outcome {
    let mut formatted_lines = vec![(Some(accessor.line()), header)];
    let (block_lines, last_processed_line) = format_block(
        &accessor.children_from(statements_offset),
        StatementFormatter::Func,
        &mut context.create_child_context(accessor.line()),
        default_surrounding_empty_lines,
    );
    formatted_lines.extend(block_lines);
    (formatted_lines, last_processed_line)
}
