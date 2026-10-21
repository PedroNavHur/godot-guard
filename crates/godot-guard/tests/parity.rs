//! The differential harness: 0.4.0 (the oracle) and this port on copies of real projects.
//!
//! Set GG_CORPUS to a directory holding `hexstead` and `wingmen` (wingmen gets the repository's
//! `examples/wingmen.godot-guard.yml`); GG_ORACLE overrides the oracle's command, by default
//! `$GG_CORPUS/oracle-venv/bin/godot-guard`. Each tool runs on a fresh copy at the same path, so only
//! timing numbers are normalized. Compared: `check`, `check --json`, `gate --fast --jobs 1` (one job, so
//! steps print in a fixed order), `baseline`, and `baseline --init --force`, by stdout, exit status, and
//! for the baseline commands the baseline file's bytes. Outputs that differ are written to
//! `$TMPDIR/godot-guard-parity/`.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use serde_json::Value;

const COMMANDS: [&[&str]; 5] = [
    &["check"],
    &["check", "--json"],
    &["gate", "--fast", "--jobs", "1"],
    &["baseline"],
    &["baseline", "--init", "--force"],
];
const BASELINE: &str = ".godot-guard-baseline.json";

/// What one command did: its exit status, its normalized stdout, and the baseline file afterwards.
#[derive(PartialEq)]
struct Outcome {
    status: i32,
    stdout: String,
    baseline: Option<Vec<u8>>,
}

fn copy_corpus(corpus: &Path, name: &str, target: &Path) {
    if target.exists() {
        fs::remove_dir_all(target).unwrap();
    }
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let status = Command::new("cp")
        .arg("-R")
        .arg(corpus.join(name))
        .arg(target)
        .status()
        .unwrap();
    assert!(status.success(), "copying {name}");
    if name == "wingmen" {
        let example =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/wingmen.godot-guard.yml");
        fs::copy(example, target.join(".godot-guard.yml")).unwrap();
    }
}

/// Run every command with `tool` on a fresh copy of `name` at `target`.
fn outcomes(tool: &Path, corpus: &Path, name: &str, target: &Path) -> Vec<Outcome> {
    copy_corpus(corpus, name, target);
    let timing = Regex::new(r"(passed in|failed after) \d+\.\ds").unwrap();
    COMMANDS
        .iter()
        .map(|args| {
            let output = Command::new(tool)
                .args(*args)
                .current_dir(target)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            Outcome {
                status: output.status.code().unwrap_or(-1),
                stdout: timing.replace_all(&stdout, "$1 X.Xs").into_owned(),
                baseline: args[0]
                    .eq("baseline")
                    .then(|| fs::read(target.join(BASELINE)).ok())
                    .flatten(),
            }
        })
        .collect()
}

/// Per checked script, its problems in `check --json`'s report, as text.
fn problems_by_file(report: &str) -> BTreeMap<String, String> {
    let report: Value = serde_json::from_str(report).expect("a JSON report");
    let mut by_file: BTreeMap<String, String> = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f.as_str().unwrap().to_owned(), String::new()))
        .collect();
    for problem in report["problems"].as_array().unwrap() {
        by_file
            .entry(problem["path"].as_str().unwrap().to_owned())
            .or_default()
            .push_str(&format!("{problem}\n"));
    }
    by_file
}

fn save(directory: &Path, label: &str, outcome: &Outcome) {
    fs::create_dir_all(directory).unwrap();
    fs::write(directory.join(format!("{label}.stdout")), &outcome.stdout).unwrap();
    fs::write(
        directory.join(format!("{label}.status")),
        outcome.status.to_string(),
    )
    .unwrap();
    if let Some(baseline) = &outcome.baseline {
        fs::write(directory.join(format!("{label}.baseline")), baseline).unwrap();
    }
}

#[test]
fn the_port_matches_0_4_0_on_the_corpus() {
    let Some(corpus) = env::var_os("GG_CORPUS").map(PathBuf::from) else {
        eprintln!("skipped: set GG_CORPUS to run the parity harness");
        return;
    };
    let oracle = env::var_os("GG_ORACLE")
        .map_or_else(|| corpus.join("oracle-venv/bin/godot-guard"), PathBuf::from);
    let port = PathBuf::from(env!("CARGO_BIN_EXE_godot-guard"));
    let work = fs::canonicalize(env::temp_dir())
        .unwrap()
        .join("godot-guard-parity");
    let mut differing = Vec::new();
    for name in ["hexstead", "wingmen"] {
        let target = work.join("copy").join(name);
        let expected = outcomes(&oracle, &corpus, name, &target);
        let actual = outcomes(&port, &corpus, name, &target);
        for ((args, want), got) in COMMANDS.iter().zip(&expected).zip(&actual) {
            let label = format!("{name}: godot-guard {}", args.join(" "));
            let same = want == got;
            eprintln!(
                "{} {label} (exit {})",
                if same { "identical" } else { "DIFFERS  " },
                want.status
            );
            if !same {
                let directory = work.join("diff").join(name).join(args.join("_"));
                save(&directory, "oracle", want);
                save(&directory, "port", got);
                differing.push(format!("{label}: see {}", directory.display()));
            }
        }
        let (want, got) = (
            problems_by_file(&expected[1].stdout),
            problems_by_file(&actual[1].stdout),
        );
        let files: Vec<&String> = want
            .keys()
            .chain(got.keys().filter(|f| !want.contains_key(*f)))
            .collect();
        let different = files
            .iter()
            .filter(|f| want.get(**f) != got.get(**f))
            .count();
        eprintln!(
            "{name}: {} files compared, {} identical, {different} differing",
            files.len(),
            files.len() - different
        );
    }
    assert!(
        differing.is_empty(),
        "outputs differ:\n{}",
        differing.join("\n")
    );
}
