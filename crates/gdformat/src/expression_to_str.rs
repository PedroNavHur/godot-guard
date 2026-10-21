use crate::expression_utils::{
    has_trailing_comma, is_any_comma, is_any_parentheses, remove_outer_parentheses,
};
use crate::function_statement_to_str::function_statement_to_str;
use crate::node::Ex;

pub fn standalone_expression_to_str(expression: &Ex) -> String {
    expression_to_str(&remove_outer_parentheses(expression))
}

pub fn expression_to_str(expression: &Ex) -> String {
    if let Some(token) = expression.token() {
        return match token.kind {
            "LONG_STRING" => long_string_to_str(&token.value),
            "LONG_RSTRING" => long_rstring_to_str(&token.value),
            "REGULAR_STRING" => regular_string_to_str(&token.value),
            "REGULAR_RSTRING" => regular_rstring_to_str(&token.value),
            _ => token.value.clone(),
        };
    }
    let e = expression;
    let str_of = |index: isize| expression_to_str(&e.child(index));
    let standalone_of = |index: isize| standalone_expression_to_str(&e.child(index));
    let joined = |separator: &str, children: &[Ex]| {
        children
            .iter()
            .map(expression_to_str)
            .collect::<Vec<_>>()
            .join(separator)
    };
    match e.data() {
        "expr" => standalone_of(0),
        "assnmnt_expr"
        | "test_expr"
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
        | "contextless_operator_chain_based_expression"
        | "test_pattern"
        | "or_pattern"
        | "and_pattern"
        | "comp_pattern"
        | "bitw_or_pattern"
        | "bitw_xor_pattern"
        | "bitw_and_pattern"
        | "shift_pattern"
        | "arith_pattern"
        | "mdr_pattern" => operator_chain_based_expression_to_str(e),
        "asless_actual_not_test" | "not_pattern" => {
            let spacing = if e.child(0).value() == "!" { "" } else { " " };
            format!("{}{spacing}{}", str_of(0), str_of(1))
        }
        "not_in_op" => "not in".to_owned(),
        "asless_actual_neg_expr" | "neg_pattern" => format!("-{}", str_of(1)),
        "asless_actual_bitw_not" | "bitw_not_pattern" => format!("~{}", str_of(1)),
        "await_expr" => {
            let children = e.children();
            let (operand, awaits) = children.split_last().expect("empty await_expr");
            let awaits: Vec<&str> = awaits.iter().map(Ex::value).collect();
            format!("{} {}", awaits.join(" "), expression_to_str(operand))
        }
        "standalone_call" | "getattr_call" | "actual_getattr_call" => call_to_str(e),
        "getattr" | "non_foldable_dot_chain" => joined("", &e.children()),
        "subscr_expr" | "actual_subscr_expr" => {
            format!("{}[{}]", str_of(0), standalone_of(1))
        }
        "par_expr" => format!("({})", standalone_of(0)),
        "array" | "array_pattern" => array_to_str(e),
        "dict" | "dict_pattern" => format!("{{{}}}", joined(", ", &e.children())),
        "c_dict_element" => format!("{}: {}", standalone_of(0), standalone_of(1)),
        "eq_dict_element" => format!("{} = {}", standalone_of(0), standalone_of(1)),
        "string" => str_of(0),
        "rstring" => format!("r{}", str_of(0)),
        "get_node" => format!("${}", str_of(0)),
        "path" => e.children().iter().map(Ex::value).collect(),
        "node_path" => format!("^{}", str_of(0)),
        "unique_node_path" => joined("", &e.children()),
        "string_name" => format!("&{}", str_of(0)),
        "lambda" => lambda_to_str(e),
        "lambda_header" => lambda_header_to_str(e),
        // fake expressions:
        "func_args" | "signal_args" => format!("({})", joined(", ", &e.children())),
        "func_arg_variadic" => format!("...{}", str_of(0)),
        "func_arg_regular" => match e.len() > 1 {
            true => format!("{} = {}", e.child(0).value(), standalone_of(1)),
            false => e.child(0).value().to_owned(),
        },
        "func_arg_inf" => format!("{} := {}", e.child(0).value(), standalone_of(1)),
        "func_arg_typed" => {
            let default = match e.len() > 2 {
                true => format!(" = {}", standalone_of(2)),
                false => String::new(),
            };
            format!("{}: {}{default}", e.child(0).value(), e.child(1).value())
        }
        "enum_body" => match e.is_empty() {
            true => "{}".to_owned(),
            false => format!("{{ {} }}", joined(", ", &e.children())),
        },
        "enum_element" => match e.len() > 1 {
            true => format!("{} = {}", standalone_of(0), standalone_of(1)),
            false => standalone_of(0),
        },
        "signal_arg_regular" => e.child(0).value().to_owned(),
        "signal_arg_typed" => format!("{}: {}", e.child(0).value(), e.child(1).value()),
        "comma_separated_list" | "contextless_comma_separated_list" => {
            arguments_to_str(&e.children())
        }
        "trailing_comma" => String::new(),
        "annotation" => annotation_to_str(e),
        "annotation_args" => annotation_args_to_str(e),
        "property_custom_getter_args" => "()".to_owned(),
        // patterns (fake expressions):
        "list_pattern" => joined(", ", &e.children()),
        "attr_pattern" => {
            let names: Vec<Ex> = e.children().into_iter().step_by(2).collect();
            joined(".", &names)
        }
        "call_pattern" => format!("{}({})", str_of(0), str_of(1)),
        "par_pattern" => format!("({})", str_of(0)),
        "var_capture_pattern" => format!("var {}", str_of(0)),
        "etc_pattern" => "..".to_owned(),
        "wildcard_pattern" => "_".to_owned(),
        "kv_pair_pattern" => format!("{}: {}", str_of(0), str_of(1)),
        other => panic!("expression_to_str: no handler for {other}"),
    }
}

fn operator_chain_based_expression_to_str(expression: &Ex) -> String {
    let children = expression.children();
    let mut text = expression_to_str(&children[0]);
    for [operator, operand] in children[1..].as_chunks::<2>().0 {
        let operator = expression_to_str(operator);
        let padding = if operator.starts_with('.') { "" } else { " " };
        text.push_str(&format!(
            "{padding}{operator}{padding}{}",
            expression_to_str(operand)
        ));
    }
    text
}

fn call_to_str(call: &Ex) -> String {
    let callee = expression_to_str(&call.child(0));
    format!("{callee}({})", arguments_to_str(&call.children_from(1)))
}

fn arguments_to_str(arguments: &[Ex]) -> String {
    arguments
        .iter()
        .filter(|argument| !is_any_parentheses(argument) && !is_any_comma(argument))
        .map(standalone_expression_to_str)
        .collect::<Vec<_>>()
        .join(", ")
}

fn annotation_to_str(annotation: &Ex) -> String {
    let name = expression_to_str(&annotation.child(0));
    if annotation.len() > 1 {
        return format!("@{name}{}", expression_to_str(&annotation.child(1)));
    }
    format!("@{name}")
}

fn annotation_args_to_str(annotation: &Ex) -> String {
    format!("({})", elements_with_trailing_comma(annotation))
}

fn array_to_str(array: &Ex) -> String {
    format!("[{}]", elements_with_trailing_comma(array))
}

fn elements_with_trailing_comma(expression: &Ex) -> String {
    let elements: Vec<String> = expression
        .children()
        .iter()
        .filter(|child| !is_any_comma(child))
        .map(standalone_expression_to_str)
        .collect();
    let trailing_comma = if has_trailing_comma(expression) {
        ","
    } else {
        ""
    };
    format!("{}{trailing_comma}", elements.join(", "))
}

fn lambda_to_str(a_lambda: &Ex) -> String {
    assert!(
        a_lambda.len() == 2,
        "lambda_to_str: lambda without one statement"
    );
    format!(
        "{} {}",
        expression_to_str(&a_lambda.child(0)),
        function_statement_to_str(&a_lambda.child(1))
    )
}

fn lambda_header_to_str(lambda_header: &Ex) -> String {
    let has_name = lambda_header.child(0).is_token();
    let optional_name = match has_name {
        true => format!(" {}", lambda_header.child(0).value()),
        false => String::new(),
    };
    let arguments = expression_to_str(&lambda_header.child(if has_name { 1 } else { 0 }));
    let len = lambda_header.len();
    let has_type_hint = len == 3 || (len == 2 && !has_name);
    let optional_type_hint = match has_type_hint {
        true => format!(
            " -> {}",
            lambda_header.child(if has_name { 2 } else { 1 }).value()
        ),
        false => String::new(),
    };
    format!("func{optional_name}{arguments}{optional_type_hint}:")
}

fn long_string_to_str(actual_string: &str) -> String {
    if actual_string.starts_with("'''")
        && actual_string.ends_with("'''")
        && actual_string.len() >= 4
    {
        return regular_string_to_str(&actual_string[2..actual_string.len() - 2]);
    }
    actual_string.to_owned()
}

fn long_rstring_to_str(actual_string: &str) -> String {
    long_string_to_str(&actual_string[1..])
}

fn regular_string_to_str(actual_string: &str) -> String {
    let mut data = actual_string[1..actual_string.len() - 1].to_owned();
    let quotes_num = data.matches('"').count();
    let aposes_num = data.matches('\'').count();
    let target = if quotes_num <= aposes_num { '"' } else { '\'' };
    if target == '"' && actual_string.starts_with('\'') {
        data = data.replace("\\'", "'").replace('"', "\\\"");
    }
    if target == '\'' && actual_string.starts_with('"') {
        data = data.replace("\\\"", "\"").replace('\'', "\\'");
    }
    format!("{target}{data}{target}")
}

fn regular_rstring_to_str(actual_string: &str) -> String {
    regular_string_to_str(&actual_string[1..])
}
