//! Port of tests/test_rules.py's comment tests.

use godot_guard::comments;

const COMMENTS: &str = include_str!("fixtures/comments.gd");

#[test]
fn only_the_long_run_after_the_header() {
    let found = comments::long_comments(COMMENTS, 2);
    assert_eq!(
        found
            .iter()
            .map(|c| (c.line, c.prose()))
            .collect::<Vec<_>>(),
        [(25, 3)]
    );
}

#[test]
fn cap_is_configurable() {
    assert!(comments::long_comments(COMMENTS, 3).is_empty());
}

#[test]
fn doc_comments_break_a_run() {
    let source = "extends Node\nvar a := 1\n# one\n# two\n## doc\n# three\n";
    assert!(comments::long_comments(source, 2).is_empty());
}

#[test]
fn header_ends_at_the_first_code_line() {
    let source = "extends Node\n## Class doc.\nvar a := 1\n# one\n# two\n# three\n";
    let lines: Vec<usize> = comments::long_comments(source, 2)
        .iter()
        .map(|c| c.line)
        .collect();
    assert_eq!(lines, [4]);
}

#[test]
fn license_text_is_exempt() {
    let source =
        "var a := 1\n# Copyright (c) 2026 Someone\n# Permission is granted\n# to anyone at all\n";
    assert!(comments::long_comments(source, 2).is_empty());
}
