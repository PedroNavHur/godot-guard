//! The safety checks on committed pairs gdtoolkit judges as noted: loosening forgives what formatting may
//! change (quotes, parentheses, a spaced minus, annotation and property placement), and nothing else.
//! `tests/fixtures/safety.jsonl` is `tools/oracle/dump_tree.py safety/*.gd`, run in `tests/fixtures/`.

use std::collections::HashMap;
use std::path::Path;

use gdformat::{ParsedCode, SafetyError, SafetyViolation, check_formatting_safety};
use gdscript::oracle::{Dump, read_line};

fn fixtures() -> HashMap<String, (String, ParsedCode)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dumps = std::fs::read_to_string(root.join("safety.jsonl")).unwrap();
    dumps
        .lines()
        .map(|line| match read_line(line).unwrap() {
            Dump::Parsed {
                path,
                tree,
                comments,
            } => {
                let code = std::fs::read_to_string(root.join(&path)).unwrap();
                let name = Path::new(&path)
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                (name, (code, ParsedCode { tree, comments }))
            }
            Dump::Rejected { path, .. } => panic!("{path} does not parse"),
        })
        .collect()
}

/// The verdict on formatting `given.gd` into the fixture `formatted`, skipping the stability check by
/// reporting it separately: these pairs are not formatter output.
fn verdict(formatted: &str) -> Result<(), SafetyViolation> {
    let mut fixtures = fixtures();
    let (given_code, given) = fixtures.remove("given").unwrap();
    let (formatted_code, formatted_trees) = fixtures.remove(formatted).unwrap();
    let result = check_formatting_safety(&given_code, &formatted_code, 100, &given, None, |_| {
        Ok::<_, ()>(formatted_trees)
    });
    match result {
        Ok(()) | Err(SafetyError::Violation(SafetyViolation::FormattingStabilityViolation)) => {
            Ok(())
        }
        Err(SafetyError::Violation(violation)) => Err(violation),
        Err(SafetyError::Parse(())) => unreachable!(),
    }
}

#[test]
fn loosening_forgives_what_formatting_changes() {
    assert_eq!(verdict("loosened"), Ok(()));
}

#[test]
fn a_changed_tree_is_a_violation() {
    assert_eq!(
        verdict("changed"),
        Err(SafetyViolation::TreeInvariantViolation)
    );
}

#[test]
fn a_dropped_comment_is_a_violation() {
    assert_eq!(
        verdict("uncommented"),
        Err(SafetyViolation::CommentPersistenceViolation {
            missing_comment: "# note".to_owned()
        })
    );
}
