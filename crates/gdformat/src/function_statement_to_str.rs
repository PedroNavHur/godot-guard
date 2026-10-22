use crate::expression_to_str::{expression_to_str, standalone_expression_to_str};
use crate::node::Ex;

/// A lambda's single statement on one line. gdtoolkit raises NotImplementedError for the statements it
/// can't put there; so does this, by panicking.
pub fn function_statement_to_str(statement: &Ex) -> String {
    let s = statement;
    let name = || s.child(0).value();
    let standalone_of = |index: isize| standalone_expression_to_str(&s.child(index));
    match s.data() {
        "pass_stmt" => "pass".to_owned(),
        "func_var_stmt" | "const_stmt" => function_statement_to_str(&s.child(0)),
        "expr_stmt" => expression_to_str(&s.child(0)), // TODO: standalone?
        "return_stmt" => format!("return {}", standalone_of(0)),
        "breakpoint_stmt" => "breakpoint".to_owned(),
        // statement fragments:
        "func_var_empty" => format!("var {}", name()),
        "func_var_assigned" => format!("var {} = {}", name(), standalone_of(1)),
        "func_var_inf" => format!("var {} := {}", name(), standalone_of(1)),
        "func_var_typed" => format!("var {}: {}", name(), standalone_of(1)),
        "func_var_typed_assgnd" | "const_typed_assigned" => {
            let keyword = if s.data() == "const_typed_assigned" {
                "const"
            } else {
                "var"
            };
            format!(
                "{keyword} {}: {} = {}",
                name(),
                s.child(1).value(),
                standalone_of(2)
            )
        }
        "const_assigned" => format!("const {} = {}", name(), standalone_of(1)),
        "const_inf" => format!("const {} := {}", name(), standalone_of(1)),
        other => panic!("function_statement_to_str: {other} is not implemented"),
    }
}
