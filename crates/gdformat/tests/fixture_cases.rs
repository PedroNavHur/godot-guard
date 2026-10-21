//! The committed fixtures under `tests/fixtures/format/` format as gdtoolkit formats them, with the same
//! safety verdicts, on the oracle's trees and on the trees `gdscript` parses. `tests/fixtures/format.jsonl` holds the oracle's cases for them; with GG_CORPUS set,
//! this also checks the file is what the oracle writes today. Regenerate it from `tests/fixtures/` with
//!
//!     $GG_CORPUS/oracle-venv/bin/python -I ../../../../tools/oracle/gdformat_cases.py --formatted-trees \
//!       --variants original,narrow,spaces4,loose,unfolded format/*.gd > format.jsonl

mod cases;

use std::path::{Path, PathBuf};
use std::process::Command;

use cases::{Verdict, compare, compare_parsed, oracle_script, read_cases};

const VARIANTS: &str = "original,narrow,spaces4,loose,unfolded";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn fixtures_format_like_gdtoolkit() {
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in read_cases(&fixtures().join("format.jsonl")) {
        let id = case.id.clone();
        let parsed = compare_parsed(&case);
        for (trees, verdict) in [("parser", parsed), ("oracle", compare(case))] {
            match verdict {
                Verdict::Identical { .. } | Verdict::BothCrashed => compared += 1,
                Verdict::Rejected => failures.push(format!("{id}: fixture does not parse")),
                Verdict::Differs { expected, actual } => failures.push(format!(
                    "{id} on {trees} trees:\n--- expected\n{expected}\n--- actual\n{actual}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(compared >= 90, "only {compared} fixture cases");
}

#[test]
fn fixture_cases_are_the_oracles() {
    let Some(corpus) = std::env::var_os("GG_CORPUS").map(PathBuf::from) else {
        return;
    };
    let mut inputs: Vec<String> = std::fs::read_dir(fixtures().join("format"))
        .unwrap()
        .map(|entry| format!("format/{}", entry.unwrap().file_name().to_string_lossy()))
        .collect();
    inputs.sort();
    let output = Command::new(corpus.join("oracle-venv/bin/python"))
        .arg("-I")
        .arg(oracle_script())
        .args(["--formatted-trees", "--variants", VARIANTS])
        .args(&inputs)
        .current_dir(fixtures())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let committed = std::fs::read(fixtures().join("format.jsonl")).unwrap();
    assert!(
        output.stdout == committed,
        "tests/fixtures/format.jsonl is stale; regenerate it"
    );
}
