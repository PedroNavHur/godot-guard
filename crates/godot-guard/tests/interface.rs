//! Port of tests/test_rules.py's interface surface test.

use godot_guard::interface;

const WIDE_NODE: &str = include_str!("fixtures/wide_node.gd");

#[test]
fn counts_exports_signals_and_setup_parameters() {
    let surface = interface::parse("wide.gd", WIDE_NODE);
    assert_eq!(surface.exports, ["color", "size", "level", "late"]);
    assert_eq!(surface.signals, ["opened", "closed"]);
    let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        surface.parameters,
        [
            ("_init".to_owned(), names(&["a", "b"])),
            ("setup".to_owned(), names(&["first", "second", "third"]))
        ]
    );
    assert_eq!(surface.total(), 11);
}
