//! Port of tests/test_cli.py, plus every argument list in tests/data/argv.jsonl, which records how
//! 0.4.0's argparse reads it (tools/oracle/argv_cases.py), checked against the Rust parser.

mod common;

use common::Repo;
use godot_guard::argparse::Exit;
use godot_guard::cli::{VERSION, build_parser, gate_jobs, jobs_argument};
use godot_guard::config::Jobs;
use serde_json::Value;

const ARGV: &str = include_str!("data/argv.jsonl");
const BOOLEANS: [&str; 5] = ["json", "init", "force", "fast", "staged"];

fn parse(argv: &[&str]) -> godot_guard::argparse::Namespace {
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    build_parser().parse(&argv, VERSION).unwrap()
}

/// The recorded namespace in the Rust parser's terms.
fn same_namespace(recorded: &Value, parsed: &godot_guard::argparse::Namespace) -> bool {
    let object = recorded.as_object().unwrap();
    let list = |key: &str| {
        object.get(key).map(|v| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
        })
    };
    let positionals: Vec<String> = list("files")
        .or_else(|| list("args"))
        .map(Iterator::collect)
        .unwrap_or_default();
    let jobs = match object.get("jobs") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(number) => Some(number.to_string()),
    };
    object["command"].as_str() == parsed.command.as_deref()
        && BOOLEANS.iter().all(|key| {
            object.get(*key).and_then(Value::as_bool).unwrap_or(false) == parsed.flag(key)
        })
        && jobs.as_ref() == parsed.values.get("jobs")
        && positionals == parsed.positionals
}

#[test]
fn every_recorded_argument_list_parses_as_0_4_0_parses_it() {
    let mut failures = Vec::new();
    for line in ARGV.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let argv: Vec<String> = case["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_owned())
            .collect();
        let parsed = build_parser().parse(&argv, VERSION);
        let matched = match (&case["ns"], parsed) {
            (
                Value::Null,
                Err(Exit {
                    status,
                    stdout,
                    stderr,
                }),
            ) => {
                let out = case["out"]
                    .as_str()
                    .unwrap()
                    .replace("godot-guard 0.4.0", VERSION);
                case["exit"] == status && out == stdout && case["err"] == stderr.as_str()
            }
            (Value::Null, Ok(_)) | (_, Err(_)) => false,
            (recorded, Ok(namespace)) => same_namespace(recorded, &namespace),
        };
        if !matched {
            failures.push(argv);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ: {failures:?}",
        failures.len(),
        ARGV.lines().count()
    );
}

#[test]
fn a_count_or_auto_is_accepted() {
    assert_eq!(jobs_argument("1").unwrap(), "1");
    assert_eq!(jobs_argument("12").unwrap(), "12");
    assert_eq!(jobs_argument("auto").unwrap(), "auto");
}

#[test]
fn anything_else_is_rejected() {
    for value in ["0", "-2", "2.5", "two", "", "auto ", "²", "+3"] {
        assert!(jobs_argument(value).is_err(), "{value:?}");
    }
}

#[test]
fn the_gate_parser_reads_it() {
    assert_eq!(parse(&["gate", "--jobs", "3"]).values["jobs"], "3");
    assert!(!parse(&["gate"]).values.contains_key("jobs"));
}

#[test]
fn the_flag_overrides_the_config() {
    let config = Repo::new(&[], Some("gate:\n  jobs: 2\n")).config();
    assert_eq!(
        gate_jobs(&config, &parse(&["gate", "--jobs", "4"])),
        Jobs::Count(4)
    );
    assert_eq!(
        gate_jobs(&config, &parse(&["gate", "--jobs", "auto"])),
        Jobs::Auto
    );
    assert_eq!(gate_jobs(&config, &parse(&["gate"])), Jobs::Count(2));
    assert_eq!(config.gate.jobs, Jobs::Count(2));
}

#[test]
fn version_and_errors_through_the_binary() {
    let repo = Repo::new(&[], None);
    let version = repo.run(&["--version"]);
    assert_eq!(
        (version.status, version.stdout.as_str()),
        (0, format!("{VERSION}\n").as_str())
    );
    let missing = repo.run(&[]);
    assert_eq!(missing.status, 2);
    assert!(
        missing
            .stderr
            .ends_with("godot-guard: error: the following arguments are required: command\n")
    );
    repo.write(".godot-guard.yml", "limits: {cognitive: 0.5}\n");
    let bad = repo.run(&["check"]);
    assert_eq!(bad.status, 2);
    assert!(
        bad.stderr.starts_with("godot-guard: .godot-guard.yml: "),
        "{}",
        bad.stderr
    );
}
