use crate::expression_utils::{
    has_trailing_comma, is_any_comma, is_any_parentheses, remove_outer_parentheses,
};
use crate::function_statement_to_str::function_statement_to_str;
use crate::node::Ex;
use gdscript::Token;

pub fn standalone_expression_to_str(expression: &Ex) -> String {
    expression_to_str(&remove_outer_parentheses(expression))
}

/// A `handlers` dict value: the text of one kind of tree.
type Handler = fn(&Ex) -> String;

/// The names the Python's `handlers` dict maps to `_operator_chain_based_expression_to_str`.
const OPERATOR_CHAIN_EXPRESSIONS: &[&str] = &[
    "assnmnt_expr",
    "test_expr",
    "asless_test_expr",
    "or_test",
    "asless_or_test",
    "and_test",
    "asless_and_test",
    "content_test",
    "asless_content_test",
    "comparison",
    "asless_comparison",
    "bitw_or",
    "asless_bitw_or",
    "bitw_xor",
    "asless_bitw_xor",
    "bitw_and",
    "asless_bitw_and",
    "shift_expr",
    "asless_shift_expr",
    "arith_expr",
    "asless_arith_expr",
    "mdr_expr",
    "asless_mdr_expr",
    "pow_expr",
    "asless_pow_expr",
    "type_test",
    "asless_type_test",
    "actual_type_cast",
    "contextless_operator_chain_based_expression",
    "test_pattern",
    "or_pattern",
    "and_pattern",
    "comp_pattern",
    "bitw_or_pattern",
    "bitw_xor_pattern",
    "bitw_and_pattern",
    "shift_pattern",
    "arith_pattern",
    "mdr_pattern",
];

/// The rest of gdtoolkit's `handlers` dict of `expression_to_str`.
const HANDLERS: &[(&str, Handler)] = &[
    ("expr", expr_to_str),
    ("asless_actual_not_test", not_test_to_str),
    ("not_pattern", not_test_to_str),
    ("not_in_op", not_in_op_to_str),
    ("asless_actual_neg_expr", neg_expr_to_str),
    ("neg_pattern", neg_expr_to_str),
    ("asless_actual_bitw_not", bitw_not_to_str),
    ("bitw_not_pattern", bitw_not_to_str),
    ("await_expr", await_expr_to_str),
    ("standalone_call", call_to_str),
    ("getattr_call", call_to_str),
    ("actual_getattr_call", call_to_str),
    ("getattr", dot_chain_to_str),
    ("non_foldable_dot_chain", dot_chain_to_str),
    ("subscr_expr", subscription_to_str),
    ("actual_subscr_expr", subscription_to_str),
    ("par_expr", par_expr_to_str),
    ("array", array_to_str),
    ("array_pattern", array_to_str),
    ("dict", dict_to_str),
    ("dict_pattern", dict_to_str),
    ("c_dict_element", c_dict_element_to_str),
    ("eq_dict_element", eq_dict_element_to_str),
    ("string", string_to_str),
    ("rstring", rstring_to_str),
    ("get_node", get_node_to_str),
    ("path", path_to_str),
    ("node_path", node_path_to_str),
    ("unique_node_path", unique_node_path_to_str),
    ("string_name", string_name_to_str),
    ("lambda", lambda_to_str),
    ("lambda_header", lambda_header_to_str),
    // fake expressions:
    ("func_args", args_to_str),
    ("signal_args", args_to_str),
    ("func_arg_variadic", func_arg_variadic_to_str),
    ("func_arg_regular", func_arg_regular_to_str),
    ("func_arg_inf", func_arg_inf_to_str),
    ("func_arg_typed", func_arg_typed_to_str),
    ("enum_body", enum_body_to_str),
    ("enum_element", enum_element_to_str),
    ("signal_arg_regular", signal_arg_regular_to_str),
    ("signal_arg_typed", signal_arg_typed_to_str),
    ("comma_separated_list", comma_separated_list_to_str),
    (
        "contextless_comma_separated_list",
        comma_separated_list_to_str,
    ),
    ("trailing_comma", trailing_comma_to_str),
    ("annotation", annotation_to_str),
    ("annotation_args", annotation_args_to_str),
    (
        "property_custom_getter_args",
        property_custom_getter_args_to_str,
    ),
    // patterns (fake expressions):
    ("list_pattern", list_pattern_to_str),
    ("attr_pattern", attr_pattern_to_str),
    ("call_pattern", call_pattern_to_str),
    ("par_pattern", par_pattern_to_str),
    ("var_capture_pattern", var_capture_pattern_to_str),
    ("etc_pattern", etc_pattern_to_str),
    ("wildcard_pattern", wildcard_pattern_to_str),
    ("kv_pair_pattern", kv_pair_pattern_to_str),
];

fn handler(data: &str) -> Option<Handler> {
    if OPERATOR_CHAIN_EXPRESSIONS.contains(&data) {
        return Some(operator_chain_based_expression_to_str);
    }
    HANDLERS
        .iter()
        .find(|(name, _)| *name == data)
        .map(|(_, handler)| *handler)
}

pub fn expression_to_str(expression: &Ex) -> String {
    if let Some(token) = expression.token() {
        return token_to_str(token);
    }
    let Some(handler) = handler(expression.data()) else {
        panic!("expression_to_str: no handler for {}", expression.data());
    };
    handler(expression)
}

/// gdtoolkit's `token_handlers`.
fn token_to_str(token: &Token) -> String {
    match token.kind {
        "LONG_STRING" => long_string_to_str(&token.value),
        "LONG_RSTRING" => long_rstring_to_str(&token.value),
        "REGULAR_STRING" => regular_string_to_str(&token.value),
        "REGULAR_RSTRING" => regular_rstring_to_str(&token.value),
        _ => token.value.clone(),
    }
}

fn str_of(expression: &Ex, index: isize) -> String {
    expression_to_str(&expression.child(index))
}

fn standalone_of(expression: &Ex, index: isize) -> String {
    standalone_expression_to_str(&expression.child(index))
}

fn joined(separator: &str, children: &[Ex]) -> String {
    children
        .iter()
        .map(expression_to_str)
        .collect::<Vec<_>>()
        .join(separator)
}

fn expr_to_str(e: &Ex) -> String {
    standalone_of(e, 0)
}

fn not_test_to_str(e: &Ex) -> String {
    let spacing = if e.child(0).value() == "!" { "" } else { " " };
    format!("{}{spacing}{}", str_of(e, 0), str_of(e, 1))
}

fn not_in_op_to_str(_: &Ex) -> String {
    "not in".to_owned()
}

fn neg_expr_to_str(e: &Ex) -> String {
    format!("-{}", str_of(e, 1))
}

fn bitw_not_to_str(e: &Ex) -> String {
    format!("~{}", str_of(e, 1))
}

fn await_expr_to_str(e: &Ex) -> String {
    let children = e.children();
    let (operand, awaits) = children.split_last().expect("empty await_expr");
    let awaits: Vec<&str> = awaits.iter().map(Ex::value).collect();
    format!("{} {}", awaits.join(" "), expression_to_str(operand))
}

fn dot_chain_to_str(e: &Ex) -> String {
    joined("", &e.children())
}

fn subscription_to_str(e: &Ex) -> String {
    format!("{}[{}]", str_of(e, 0), standalone_of(e, 1))
}

fn par_expr_to_str(e: &Ex) -> String {
    format!("({})", standalone_of(e, 0))
}

fn dict_to_str(e: &Ex) -> String {
    format!("{{{}}}", joined(", ", &e.children()))
}

fn c_dict_element_to_str(e: &Ex) -> String {
    format!("{}: {}", standalone_of(e, 0), standalone_of(e, 1))
}

fn eq_dict_element_to_str(e: &Ex) -> String {
    format!("{} = {}", standalone_of(e, 0), standalone_of(e, 1))
}

fn string_to_str(e: &Ex) -> String {
    str_of(e, 0)
}

fn rstring_to_str(e: &Ex) -> String {
    format!("r{}", str_of(e, 0))
}

fn get_node_to_str(e: &Ex) -> String {
    format!("${}", str_of(e, 0))
}

fn path_to_str(e: &Ex) -> String {
    e.children().iter().map(Ex::value).collect()
}

fn node_path_to_str(e: &Ex) -> String {
    format!("^{}", str_of(e, 0))
}

fn unique_node_path_to_str(e: &Ex) -> String {
    joined("", &e.children())
}

fn string_name_to_str(e: &Ex) -> String {
    format!("&{}", str_of(e, 0))
}

fn args_to_str(e: &Ex) -> String {
    format!("({})", joined(", ", &e.children()))
}

fn func_arg_variadic_to_str(e: &Ex) -> String {
    format!("...{}", str_of(e, 0))
}

fn func_arg_regular_to_str(e: &Ex) -> String {
    if e.len() > 1 {
        return format!("{} = {}", e.child(0).value(), standalone_of(e, 1));
    }
    e.child(0).value().to_owned()
}

fn func_arg_inf_to_str(e: &Ex) -> String {
    format!("{} := {}", e.child(0).value(), standalone_of(e, 1))
}

fn func_arg_typed_to_str(e: &Ex) -> String {
    let default = match e.len() > 2 {
        true => format!(" = {}", standalone_of(e, 2)),
        false => String::new(),
    };
    format!("{}: {}{default}", e.child(0).value(), e.child(1).value())
}

fn enum_body_to_str(e: &Ex) -> String {
    if e.is_empty() {
        return "{}".to_owned();
    }
    format!("{{ {} }}", joined(", ", &e.children()))
}

fn enum_element_to_str(e: &Ex) -> String {
    if e.len() > 1 {
        return format!("{} = {}", standalone_of(e, 0), standalone_of(e, 1));
    }
    standalone_of(e, 0)
}

fn signal_arg_regular_to_str(e: &Ex) -> String {
    e.child(0).value().to_owned()
}

fn signal_arg_typed_to_str(e: &Ex) -> String {
    format!("{}: {}", e.child(0).value(), e.child(1).value())
}

fn comma_separated_list_to_str(e: &Ex) -> String {
    arguments_to_str(&e.children())
}

fn trailing_comma_to_str(_: &Ex) -> String {
    String::new()
}

fn property_custom_getter_args_to_str(_: &Ex) -> String {
    "()".to_owned()
}

fn list_pattern_to_str(e: &Ex) -> String {
    joined(", ", &e.children())
}

fn attr_pattern_to_str(e: &Ex) -> String {
    let names: Vec<Ex> = e.children().into_iter().step_by(2).collect();
    joined(".", &names)
}

fn call_pattern_to_str(e: &Ex) -> String {
    format!("{}({})", str_of(e, 0), str_of(e, 1))
}

fn par_pattern_to_str(e: &Ex) -> String {
    format!("({})", str_of(e, 0))
}

fn var_capture_pattern_to_str(e: &Ex) -> String {
    format!("var {}", str_of(e, 0))
}

fn etc_pattern_to_str(_: &Ex) -> String {
    "..".to_owned()
}

fn wildcard_pattern_to_str(_: &Ex) -> String {
    "_".to_owned()
}

fn kv_pair_pattern_to_str(e: &Ex) -> String {
    format!("{}: {}", str_of(e, 0), str_of(e, 1))
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
