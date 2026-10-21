use crate::context::{Context, ExpressionContext};
use crate::expression::format_expression;
use crate::node::Ex;
use crate::statement_utils::format_simple_statement;
use crate::types::Outcome;

pub fn format_var_statement<'a>(
    statement: &Ex<'a>,
    context: &Context<'a>,
    prefix: &str,
) -> Outcome {
    let concrete_var_stmt = statement.child(0);
    let s = &concrete_var_stmt;
    let name = s.child(0).value();
    let assigned = |head: String, expr: Ex<'a>| {
        let expression_context = ExpressionContext::new(head, s.line(), "", s.end_line());
        format_expression(&expr, &expression_context, context)
    };
    match s.data() {
        "class_var_empty" | "func_var_empty" => {
            format_simple_statement(&format!("{prefix}var {name}"), s, context)
        }
        "class_var_typed" | "func_var_typed" => {
            let line = format!("{prefix}var {name}: {}", s.child(1).value());
            format_simple_statement(&line, s, context)
        }
        "class_var_assigned" | "func_var_assigned" => {
            assigned(format!("{prefix}var {name} = "), s.child(1))
        }
        "class_var_inf" | "func_var_inf" => assigned(format!("{prefix}var {name} := "), s.child(1)),
        "class_var_typed_assgnd" | "func_var_typed_assgnd" => assigned(
            format!("{prefix}var {name}: {} = ", s.child(1).value()),
            s.child(2),
        ),
        other => panic!("format_var_statement: no handler for {other}"),
    }
}
