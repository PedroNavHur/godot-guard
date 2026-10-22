use crate::context::{Context, ExpressionContext};
use crate::expression::format_expression;
use crate::node::Ex;
use crate::types::Outcome;

pub fn format_const_statement<'a>(statement: &Ex<'a>, context: &Context<'a>) -> Outcome {
    let concrete_const_stmt = statement.child(0);
    let s = &concrete_const_stmt;
    let name = s.child(0).value();
    let prefix = match s.data() {
        "const_assigned" => format!("const {name} = "),
        "const_typed_assigned" => format!("const {name}: {} = ", s.child(1).value()),
        "const_inf" => format!("const {name} := "),
        other => panic!("format_const_statement: no handler for {other}"),
    };
    let expression_context = ExpressionContext::new(prefix, s.line(), "", s.end_line());
    format_expression(&s.child(-1), &expression_context, context)
}
