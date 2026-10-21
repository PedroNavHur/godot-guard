//! `gdlint::pyre` against Python's `re.fullmatch`, recorded by `tools/oracle/pyre_cases.py`.

use gdlint::pyre::FullMatch;
use serde_json::Value;

/// Patterns Python accepts that the port refuses, loudly, because `regex` cannot express them.
const UNSUPPORTED: [&str; 10] = [
    r"(a)\1",
    "(?=a)a",
    "(?<=a)b",
    "(?x)a b",
    r"(?a)\w",
    "a(?i)",
    r"\0",
    "(?i)[a-z]+",
    "(?i:A)b",
    r"\bA",
];

#[test]
fn name_patterns_match_as_python_re_does() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/pyre_cases.json")).expect("fixture");
    let names: Vec<&str> = fixture["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    let mut failures = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let pattern = case["pattern"].as_str().unwrap();
        let compiled = FullMatch::new(pattern);
        if case["error"].as_bool().unwrap() {
            if compiled.is_ok() {
                failures.push(format!(
                    "{pattern:?}: Python refuses it ({}), the port accepts it",
                    case["reason"]
                ));
            }
            continue;
        }
        let compiled = match compiled {
            Ok(compiled) => compiled,
            Err(_) if UNSUPPORTED.contains(&pattern) => continue,
            Err(error) => {
                failures.push(format!(
                    "{pattern:?}: Python accepts it, the port refuses it: {error}"
                ));
                continue;
            }
        };
        assert!(
            !UNSUPPORTED.contains(&pattern),
            "{pattern:?} is listed unsupported but compiles"
        );
        let expected = case["matches"].as_str().unwrap().chars().map(|c| c == '1');
        for (name, expected) in names.iter().zip(expected) {
            if compiled.is_match(name) != expected {
                failures.push(format!("{pattern:?} on {name:?}: Python says {expected}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreements:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
