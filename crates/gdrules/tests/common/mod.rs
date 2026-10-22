//! Read `tools/oracle/dump_rules.py` output and compare the Rust rules with the Python's on the same tree.

#![allow(dead_code, reason = "each test binary uses part of this module")]

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use gdrules::{complexity, patterns};
use gdscript::Tree;
use serde_json::{Value, json};

/// One dump line: the tree the Python ran on, and what it produced.
pub struct Case {
    pub path: String,
    pub tree: Option<Tree>,
    pub value: Value,
}

pub fn read_line(line: &str) -> Case {
    let mut reader = serde_json::Deserializer::from_str(line);
    reader.disable_recursion_limit();
    let value: Value = serde::Deserialize::deserialize(&mut reader).expect("dump line is JSON");
    let path = value["path"]
        .as_str()
        .expect("dump line has a path")
        .to_owned();
    let tree = value.get("tree").map(|tree| {
        match gdscript::oracle::node(tree).expect("dump tree loads") {
            gdscript::Node::Tree(tree) => tree,
            gdscript::Node::Token(_) => panic!("{path}: dump root is a token"),
        }
    });
    Case { path, tree, value }
}

/// The Rust results in the oracle's `rules` shape.
pub fn rules(tree: &Tree) -> Value {
    let functions: Vec<Value> = complexity::score(tree)
        .into_iter()
        .map(|f| json!([f.name, f.line, f.end_line, f.cognitive, f.cyclomatic]))
        .collect();
    let patterns: Vec<Value> = patterns::scan(tree)
        .iter()
        .map(|(rule, hits)| json!([rule, hits]))
        .collect();
    json!({
        "functions": functions,
        "nested_ternaries": complexity::nested_ternaries(tree),
        "patterns": patterns,
    })
}

#[derive(Debug, Default)]
pub struct Tally {
    pub compared: usize,
    pub identical: usize,
    pub rejected: usize,
    pub functions: usize,
    /// Pattern hits the oracle reported, by rule.
    pub hits: BTreeMap<String, usize>,
    pub differing: Vec<String>,
}

impl Tally {
    /// Compare one case; a file the Python crashed on must make the Rust panic too.
    pub fn add(&mut self, case: &Case) {
        let Some(tree) = &case.tree else {
            self.rejected += 1;
            return;
        };
        self.compared += 1;
        let actual = catch_unwind(AssertUnwindSafe(|| rules(tree)));
        let same = match (&actual, case.value.get("rules")) {
            (Ok(actual), Some(expected)) => {
                self.functions += expected["functions"].as_array().map_or(0, Vec::len);
                count_hits(expected, &mut self.hits);
                for key in ["functions", "nested_ternaries", "patterns"] {
                    if actual[key] != expected[key] {
                        self.differing.push(format!(
                            "{}: {key}\n  python: {}\n  rust:   {}",
                            case.path, expected[key], actual[key]
                        ));
                    }
                }
                actual == expected
            }
            (Err(_), None) => true,
            (Ok(_), None) => {
                self.differing.push(format!(
                    "{}: python crashed ({}), rust did not",
                    case.path, case.value["crash"]
                ));
                false
            }
            (Err(_), Some(_)) => {
                self.differing
                    .push(format!("{}: rust panicked, python did not", case.path));
                false
            }
        };
        self.identical += usize::from(same);
    }

    pub fn summary(&self) -> String {
        format!(
            "{} files compared, {} identical, {} differing, {} rejected by the parser; {} functions",
            self.compared,
            self.identical,
            self.compared - self.identical,
            self.rejected,
            self.functions
        )
    }
}

fn count_hits(rules: &Value, hits: &mut BTreeMap<String, usize>) {
    for rule in rules["patterns"].as_array().into_iter().flatten() {
        let found = rule[1].as_array().map_or(0, Vec::len);
        *hits
            .entry(rule[0].as_str().unwrap_or_default().to_owned())
            .or_default() += found;
    }
}
