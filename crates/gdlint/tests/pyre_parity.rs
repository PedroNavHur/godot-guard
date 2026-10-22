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
    let failures: Vec<String> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|case| case_failures(case, &names))
        .collect();
    assert!(
        failures.is_empty(),
        "{} disagreements:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Where the port and Python disagree about one fixture case: whether the pattern compiles, and which names
/// it matches.
fn case_failures(case: &Value, names: &[&str]) -> Vec<String> {
    let pattern = case["pattern"].as_str().unwrap();
    let compiled = FullMatch::new(pattern);
    if case["error"].as_bool().unwrap() {
        return refused_case_failures(case, pattern, compiled.is_ok());
    }
    let compiled = match compiled {
        Ok(compiled) => compiled,
        Err(_) if UNSUPPORTED.contains(&pattern) => return Vec::new(),
        Err(error) => {
            return vec![format!(
                "{pattern:?}: Python accepts it, the port refuses it: {error}"
            )];
        }
    };
    assert!(
        !UNSUPPORTED.contains(&pattern),
        "{pattern:?} is listed unsupported but compiles"
    );
    let expected = case["matches"].as_str().unwrap().chars().map(|c| c == '1');
    names
        .iter()
        .zip(expected)
        .filter(|(name, expected)| compiled.is_match(name) != *expected)
        .map(|(name, expected)| format!("{pattern:?} on {name:?}: Python says {expected}"))
        .collect()
}

/// The failure of a case Python refuses, if the port accepts it.
fn refused_case_failures(case: &Value, pattern: &str, port_accepts: bool) -> Vec<String> {
    if !port_accepts {
        return Vec::new();
    }
    vec![format!(
        "{pattern:?}: Python refuses it ({}), the port accepts it",
        case["reason"]
    )]
}
