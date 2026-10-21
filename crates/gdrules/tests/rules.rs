//! The cases of godot-guard's `tests/test_complexity.py` and `tests/test_patterns.py`, on the oracle's trees
//! of `tests/fixtures/*.gd`.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use gdrules::complexity::{nested_ternaries, score};
use gdrules::patterns::{ADVISORY, COUNTED, scan};
use gdscript::Tree;

fn tree(name: &str) -> Tree {
    let dump = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixtures.jsonl");
    let text = std::fs::read_to_string(dump).expect("read fixture dump");
    let path = format!("tests/fixtures/{name}.gd");
    let case = text
        .lines()
        .map(common::read_line)
        .find(|case| case.path == path);
    case.and_then(|case| case.tree)
        .unwrap_or_else(|| panic!("{path} is not in the fixture dump"))
}

fn lines(name: &str) -> BTreeMap<&'static str, Vec<usize>> {
    let hits = scan(&tree(name));
    hits.iter()
        .map(|(rule, hits)| (rule, hits.iter().map(|(line, _)| *line).collect()))
        .collect()
}

#[test]
fn scores_every_function() {
    let scores: BTreeMap<String, (usize, Option<usize>, usize)> = score(&tree("complexity"))
        .into_iter()
        .map(|f| (f.name, (f.line, f.cognitive, f.cyclomatic)))
        .collect();
    let expected = [
        ("health.set", (4, Some(4), 4)),
        ("health.get", (9, Some(0), 1)),
        ("chain", (13, Some(4), 4)),
        ("matcher", (24, Some(3), 5)),
        ("with_lambda", (36, Some(3), 1)),
        ("with_lambda.<lambda#1>", (37, None, 3)),
        ("loops", (40, Some(6), 4)),
        ("recurse", (49, Some(2), 2)),
        ("Inner.helper", (56, Some(3), 5)),
    ];
    assert_eq!(
        scores,
        expected
            .into_iter()
            .map(|(name, s)| (name.to_owned(), s))
            .collect()
    );
}

#[test]
fn lambda_folds_into_enclosing_cognitive() {
    let functions = score(&tree("lambda_folds"));
    let by_name = |name: &str| {
        functions
            .iter()
            .find(|f| f.name == name)
            .expect("function scored")
    };
    assert_eq!(by_name("f").cognitive, Some(2));
    assert_eq!(by_name("f.<lambda#1>").cyclomatic, 2);
}

#[test]
fn lambdas_are_named_after_their_enclosing_function() {
    let names: Vec<String> = score(&tree("lambda_names"))
        .into_iter()
        .map(|f| f.name)
        .collect();
    let expected = [
        "<lambda#1>",
        "f",
        "f.<lambda#1>",
        "f.<lambda#1>.<lambda#1>",
        "f.named",
        "f.<lambda#2>",
        "Inner.<lambda#1>",
    ];
    assert_eq!(names, expected);
}

#[test]
fn nested_ternary_reports_outermost_once() {
    assert_eq!(nested_ternaries(&tree("nested_ternary")), [6, 7, 8]);
}

#[test]
fn flat_ternaries_are_not_nested() {
    assert!(nested_ternaries(&tree("flat_ternaries")).is_empty());
}

#[test]
fn hot_path_rules() {
    let found = lines("hot_path");
    assert_eq!(found["lookup-in-hot-path"], [12, 13, 14, 15, 24, 29, 41]);
    assert_eq!(found["load-in-hot-path"], [16, 17]);
    assert_eq!(found["per-frame-allocation"], [18, 19]);
    assert_eq!(found["private-export"], [3, 6, 38]);
    assert_eq!(found["self-assign"], [21, 22]);
}

#[test]
fn messages_name_the_call_and_the_function() {
    let hits = scan(&tree("hot_path"));
    let message = |line: usize| {
        &hits["lookup-in-hot-path"]
            .iter()
            .find(|(at, _)| *at == line)
            .expect("hit")
            .1
    };
    assert!(message(12).contains("$path inside _process()"));
    assert!(message(41).contains("get_node_or_null() inside _physics_process()"));
}

#[test]
fn scan_reports_every_counted_and_advisory_rule_in_order() {
    let rules: Vec<&str> = scan(&tree("runner")).iter().map(|(rule, _)| rule).collect();
    assert_eq!(
        rules,
        [
            "lookup-in-hot-path",
            "load-in-hot-path",
            "per-frame-allocation",
            "private-export",
            "self-assign"
        ]
    );
    let mut listed: Vec<&str> = COUNTED
        .iter()
        .map(|(rule, _)| *rule)
        .chain(ADVISORY)
        .collect();
    listed.sort();
    let mut scanned = rules.clone();
    scanned.sort();
    assert_eq!(listed, scanned);
}

#[test]
fn runner_scripts_are_exempt_from_the_hot_path_rules() {
    assert!(lines("runner").values().all(Vec::is_empty));
}

#[test]
fn a_runner_keeps_its_exemption_past_an_inner_class_with_another_base() {
    assert!(lines("runner_inner_class")["lookup-in-hot-path"].is_empty());
}

#[test]
fn a_class_name_runner_is_exempt() {
    assert!(lines("class_name_runner")["lookup-in-hot-path"].is_empty());
}

#[test]
fn an_inner_runner_does_not_exempt_the_script() {
    assert_eq!(lines("inner_runner")["lookup-in-hot-path"], [8]);
}

#[test]
fn a_new_hot_function_adds_a_lookup() {
    assert_eq!(lines("hot_path_input")["lookup-in-hot-path"].len(), 8);
}
