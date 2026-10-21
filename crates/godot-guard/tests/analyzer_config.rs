//! Port of tests/test_analyzer_config.py's rule tests; its check wiring tests are in tests/check.rs.

mod common;

use common::{Repo, strict_project};
use godot_guard::analyzer_config;
use godot_guard::rules::Problem;

const SCRIPT: (&str, &str) = ("scripts/a.gd", "extends Node\n");

const HEXSTEAD_WARNINGS: [&str; 43] = [
    "assert_always_false",
    "assert_always_true",
    "confusable_capture_reassignment",
    "confusable_identifier",
    "confusable_local_declaration",
    "confusable_local_usage",
    "confusable_temporary_modification",
    "constant_used_as_function",
    "deprecated_keyword",
    "empty_file",
    "enum_variable_without_default",
    "function_used_as_property",
    "incompatible_ternary",
    "int_as_enum_without_cast",
    "int_as_enum_without_match",
    "integer_division",
    "missing_await",
    "missing_tool",
    "narrowing_conversion",
    "property_used_as_function",
    "redundant_await",
    "redundant_static_unload",
    "shadowed_global_identifier",
    "shadowed_variable",
    "shadowed_variable_base_class",
    "standalone_expression",
    "standalone_ternary",
    "static_called_on_instance",
    "unassigned_variable",
    "unassigned_variable_op_assign",
    "unreachable_code",
    "unreachable_pattern",
    "unsafe_call_argument",
    "unsafe_cast",
    "unsafe_method_access",
    "unsafe_property_access",
    "unsafe_void_return",
    "untyped_declaration",
    "unused_local_constant",
    "unused_parameter",
    "unused_private_class_variable",
    "unused_signal",
    "unused_variable",
];

/// The strict project with each `key=value` in `overrides` replacing that key's setting.
fn project(overrides: &[&str]) -> String {
    let key = |line: &str| line.split('=').next().unwrap_or("").to_owned();
    let keys: Vec<String> = overrides.iter().map(|line| key(line)).collect();
    let strict = strict_project();
    let kept = strict.lines().filter(|line| !keys.contains(&key(line)));
    let mut lines: Vec<&str> = kept.collect();
    lines.extend(overrides);
    lines.join("\n") + "\n"
}

/// A project with hexstead's warnings each at error, except `skipped`.
fn hexstead_project(skipped: &str) -> String {
    let settings: String = HEXSTEAD_WARNINGS
        .iter()
        .filter(|name| **name != skipped)
        .map(|name| format!("gdscript/warnings/{name}=2\n"))
        .collect();
    format!("config_version=5\n\n[debug]\n\n{settings}")
}

fn problems(project_text: &str, config_text: Option<&str>) -> Vec<Problem> {
    let repo = Repo::new(&[SCRIPT, ("project.godot", project_text)], config_text);
    analyzer_config::problems(&repo.config()).unwrap()
}

fn one(project_text: &str) -> Problem {
    let mut found = problems(project_text, None);
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

fn line_of(text: &str, line: &str) -> usize {
    text.lines().position(|l| l == line).unwrap() + 1
}

#[test]
fn every_required_warning_at_error_passes() {
    assert!(problems(&strict_project(), None).is_empty());
}

#[test]
fn a_bare_project_lists_the_warnings_below_error_by_default() {
    let problem = one("config_version=5\n");
    assert_eq!(
        (problem.path.as_str(), problem.line, problem.rule),
        ("project.godot", 1, "analyzer-config")
    );
    assert!(problem.message.contains("untyped_declaration"));
    assert!(problem.message.contains("unused_variable"));
    assert!(!problem.message.contains("native_method_override"));
}

#[test]
fn the_missing_list_points_at_the_debug_header() {
    let problem = one("config_version=5\n\n[debug]\n\ngdscript/warnings/unused_variable=2\n");
    assert_eq!(problem.line, 3);
    assert!(!problem.message.contains("unused_variable"));
}

#[test]
fn a_warning_below_error_is_reported_on_its_line() {
    let text = project(&["gdscript/warnings/unsafe_cast=1"]);
    let problem = one(&text);
    assert_eq!(
        problem.line,
        line_of(&text, "gdscript/warnings/unsafe_cast=1")
    );
    assert!(
        problem
            .message
            .contains("gdscript/warnings/unsafe_cast is 1")
    );
}

#[test]
fn lowering_a_warning_godot_defaults_to_error_is_reported() {
    let problem = one(&project(&["gdscript/warnings/native_method_override=1"]));
    assert!(problem.message.contains("native_method_override is 1"));
}

#[test]
fn turning_warnings_off_is_reported() {
    assert!(
        one(&project(&["gdscript/warnings/enable=false"]))
            .message
            .contains("gdscript/warnings/enable")
    );
}

#[test]
fn settings_outside_the_debug_section_do_not_count() {
    let problem =
        one("config_version=5\n\n[application]\n\ngdscript/warnings/untyped_declaration=2\n");
    assert!(problem.message.contains("untyped_declaration"));
}

#[test]
fn relaxed_warnings_are_not_checked() {
    let config_text = "relaxed-warnings: [unsafe_cast, untyped_declaration]\n";
    let text = project(&[
        "gdscript/warnings/unsafe_cast=1",
        "gdscript/warnings/untyped_declaration=0",
    ]);
    assert!(problems(&text, Some(config_text)).is_empty());
}

#[test]
fn a_relaxed_name_outside_the_required_set_is_a_config_error() {
    let repo = Repo::new(
        &[SCRIPT],
        Some("relaxed-warnings: [inferred_declaration]\n"),
    );
    assert!(repo.config_error().contains("inferred_declaration"));
}

#[test]
fn hexsteads_settings_at_error_pass() {
    assert!(problems(&hexstead_project(""), None).is_empty());
}

#[test]
fn dropping_one_of_hexsteads_settings_names_it() {
    for name in HEXSTEAD_WARNINGS {
        assert!(
            one(&hexstead_project(name)).message.contains(name),
            "{name}"
        );
    }
}

#[test]
fn default_addons_directory_rule_passes() {
    assert!(
        problems(
            &project(&[r#"gdscript/warnings/directory_rules={"res://addons": 0}"#]),
            None
        )
        .is_empty()
    );
    let multi_line = project(&["gdscript/warnings/directory_rules={\n\"res://addons\": 0\n}"]);
    assert!(problems(&multi_line, None).is_empty());
}

#[test]
fn a_directory_rule_turning_warnings_off_under_scripts_is_reported() {
    let text = project(&[
        "gdscript/warnings/directory_rules={\n\"res://addons\": 0,\n\"res://scripts\": 0\n}",
    ]);
    let problem = one(&text);
    assert_eq!(
        problem.line,
        line_of(&text, "gdscript/warnings/directory_rules={")
    );
    assert!(problem.message.contains("res://scripts"));
}

#[test]
fn a_one_line_directory_rule_under_scripts_is_reported() {
    let problem = one(&project(&[
        r#"gdscript/warnings/directory_rules={"res://scripts": 0}"#,
    ]));
    assert!(problem.message.contains("res://scripts"));
    let spaced = one(&project(&[
        r#"gdscript/warnings/directory_rules = {"res://scripts": 0}"#,
    ]));
    assert!(spaced.message.contains("res://scripts"));
}

#[test]
fn a_directory_rule_that_keeps_warnings_passes() {
    assert!(
        problems(
            &project(&[r#"gdscript/warnings/directory_rules={"res://scripts": 1}"#]),
            None
        )
        .is_empty()
    );
}
