use crate::context::{Context, ExpressionContext};
use crate::expression::format_concrete_expression;
use crate::expression_to_str::expression_to_str;
use crate::node::Ex;
use crate::types::{FormattedLine, FormattedLines, Outcome};
use pycompat::{builtins, string};

const STANDALONE_ANNOTATIONS: [&str; 8] = [
    "abstract",
    "export_category",
    "export_group",
    "export_subgroup",
    "icon",
    "tool",
    "warning_ignore_start",
    "warning_ignore_restore",
];

/// The warnings a `@warning_ignore` can share a line with the function it precedes. From gdtoolkit,
/// which took them from Godot's `modules/gdscript/gdscript_warning.h` (2025-01-09), leaving out the
/// warnings that can't apply to functions and the deprecated ones.
const NON_STANDALONE_WARNING_IGNORES: [&str; 35] = [
    "unassigned_variable",
    "unassigned_variable_op_assign",
    "unused_parameter",
    "shadowed_global_identifier",
    "shadowed_variable",
    "shadowed_variable_base_class",
    "unreachable_code",
    "unreachable_pattern",
    "standalone_expression",
    "standalone_ternary",
    "incompatible_ternary",
    "untyped_declaration",
    "inferred_declaration",
    "unsafe_property_access",
    "unsafe_method_access",
    "unsafe_cast",
    "unsafe_call_argument",
    "unsafe_void_return",
    "return_value_discarded",
    "static_called_on_instance",
    "redundant_await",
    "assert_always_true",
    "assert_always_false",
    "integer_division",
    "narrowing_conversion",
    "int_as_enum_without_cast",
    "int_as_enum_without_match",
    "enum_variable_without_default",
    "deprecated_keyword",
    "confusable_identifier",
    "confusable_local_declaration",
    "confusable_local_usage",
    "confusable_capture_reassignment",
    "inference_on_variant",
    "native_method_override",
];

/// An `@abstract` annotation to be combined with the next statement.
pub fn is_abstract_annotation_for_statement(statement: &Ex, next_statement: &Ex) -> bool {
    statement.data() == "annotation"
        && statement.child(0).value() == "abstract"
        && matches!(
            next_statement.data(),
            "abstract_func_def" | "classname_stmt" | "class_def"
        )
}

pub fn is_non_standalone_annotation(statement: &Ex) -> bool {
    if statement.data() != "annotation" {
        return false;
    }
    let name = statement.child(0).value();
    if STANDALONE_ANNOTATIONS.contains(&name) {
        return false;
    }
    if name != "warning_ignore" {
        return true;
    }
    let ignoree = statement
        .child(1)
        .child(0)
        .child(0)
        .value()
        .trim_matches('"');
    NON_STANDALONE_WARNING_IGNORES.contains(&ignoree)
}

pub fn prepend_annotations_to_formatted_line<'a>(
    line_to_prepend_to: FormattedLine,
    context: &mut Context<'a>,
) -> FormattedLines {
    assert!(!context.annotations.is_empty());
    let annotations = std::mem::take(&mut context.annotations);
    let whitelineless_line = string::strip(&line_to_prepend_to.1);
    let annotations_string = annotations
        .iter()
        .map(expression_to_str)
        .collect::<Vec<_>>()
        .join(" ");
    let single_line_length = context.indent as i64
        + builtins::len(&annotations_string)
        + builtins::len(whitelineless_line);
    // Check if this is an abstract function or class_name annotation
    let lone_abstract = annotations.len() == 1 && annotations[0].child(0).value() == "abstract";
    let is_abstract_func = lone_abstract && whitelineless_line.starts_with("func");
    let is_abstract_class_name = lone_abstract && whitelineless_line.starts_with("class_name");
    let standalone_formatting_enforced = (whitelineless_line.starts_with("func")
        || whitelineless_line.starts_with("static func"))
        && !is_abstract_func
        && !is_abstract_class_name;
    if !annotations_have_standalone_comments(
        &annotations,
        context.standalone_comments,
        line_to_prepend_to.0,
    ) && single_line_length <= context.max_line_length
        && !standalone_formatting_enforced
    {
        let single_line = format!(
            "{}{annotations_string} {whitelineless_line}",
            context.indent_string
        );
        return vec![(line_to_prepend_to.0, single_line)];
    }
    let mut formatted_lines = FormattedLines::new();
    for annotation in &annotations {
        formatted_lines.extend(format_standalone_annotation(annotation, context).0);
    }
    formatted_lines.push(line_to_prepend_to);
    formatted_lines
}

pub fn format_standalone_annotation<'a>(annotation: &Ex<'a>, context: &Context<'a>) -> Outcome {
    format_concrete_expression(
        annotation,
        &ExpressionContext::new("", annotation.line(), "", -1),
        context,
    )
}

fn annotations_have_standalone_comments(
    annotations: &[Ex],
    standalone_comments: &[Option<String>],
    last_line: Option<i64>,
) -> bool {
    let end = last_line.unwrap_or_else(|| annotations[annotations.len() - 1].end_line());
    let range = builtins::slice(
        standalone_comments.len(),
        Some(annotations[0].line()),
        Some(end),
    );
    standalone_comments[range].iter().any(Option::is_some)
}
