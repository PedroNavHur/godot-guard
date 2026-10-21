//! The GDScript analyzer warnings `analyzer-config` requires at error, with Godot 4.7.1's default level for each.
//!
//! hexstead's set, plus the four Godot already defaults to error. Left out on purpose:
//! `inferred_declaration` (forbids `:=`) and `return_value_discarded` (fires on most calls).

pub const ERROR: u8 = 2;

/// (name, default level), in 0.4.0's order.
pub const REQUIRED: &[(&str, u8)] = &[
    ("assert_always_false", 1),
    ("assert_always_true", 1),
    ("confusable_capture_reassignment", 1),
    ("confusable_identifier", 1),
    ("confusable_local_declaration", 1),
    ("confusable_local_usage", 1),
    ("confusable_temporary_modification", 1),
    ("constant_used_as_function", 1),
    ("deprecated_keyword", 1),
    ("empty_file", 1),
    ("enum_variable_without_default", 1),
    ("function_used_as_property", 1),
    ("get_node_default_without_onready", 2),
    ("incompatible_ternary", 1),
    ("inference_on_variant", 2),
    ("int_as_enum_without_cast", 1),
    ("int_as_enum_without_match", 1),
    ("integer_division", 1),
    ("missing_await", 0),
    ("missing_tool", 1),
    ("narrowing_conversion", 1),
    ("native_method_override", 2),
    ("onready_with_export", 2),
    ("property_used_as_function", 1),
    ("redundant_await", 1),
    ("redundant_static_unload", 1),
    ("shadowed_global_identifier", 1),
    ("shadowed_variable", 1),
    ("shadowed_variable_base_class", 1),
    ("standalone_expression", 1),
    ("standalone_ternary", 1),
    ("static_called_on_instance", 1),
    ("unassigned_variable", 1),
    ("unassigned_variable_op_assign", 1),
    ("unreachable_code", 1),
    ("unreachable_pattern", 1),
    ("unsafe_call_argument", 0),
    ("unsafe_cast", 0),
    ("unsafe_method_access", 0),
    ("unsafe_property_access", 0),
    ("unsafe_void_return", 1),
    ("untyped_declaration", 0),
    ("unused_local_constant", 1),
    ("unused_parameter", 1),
    ("unused_private_class_variable", 1),
    ("unused_signal", 1),
    ("unused_variable", 1),
];

pub fn is_required(name: &str) -> bool {
    REQUIRED.iter().any(|(required, _)| *required == name)
}
