use crate::context::Context;
use crate::node::Ex;
use crate::types::Outcome;

pub fn format_simple_statement(statement_name: &str, statement: &Ex, context: &Context) -> Outcome {
    (
        vec![(
            Some(statement.line()),
            format!("{}{statement_name}", context.indent_string),
        )],
        statement.line(),
    )
}
