//! Port of tests/test_json.py's report tests and the check wiring tests of test_config.py and
//! test_analyzer_config.py. The tests that need stderr or the command line are in tests/cli.rs.

mod common;

use common::{Repo, branches, check, check_json, record};
use serde_json::{Value, json};

const JSON_CONFIG: &str = "exclude: [vendor]\nlimits:\n  cognitive: 2\n  comment-lines: 3\nrules:\n  cyclomatic-complexity: warn\n  nested-ternary: off\n";

fn json_repo() -> Repo {
    let busy = format!("extends Node\n\n\n{}", branches("busy", 4));
    Repo::new(
        &[
            ("a.gd", &busy),
            ("b.gd", "extends Node\n"),
            ("vendor/c.gd", &busy),
        ],
        Some(JSON_CONFIG),
    )
}

#[test]
fn json_reports_settings_files_and_problems() {
    let repo = json_repo();
    let (status, report) = check_json(&repo.config(), None);
    assert_eq!(status, 1);
    assert_eq!(report["version"], 1);
    assert_eq!(report["root"], repo.root.to_string_lossy().as_ref());
    assert_eq!(
        report["limits"],
        json!({"cognitive": 2, "cyclomatic": 16, "comment-lines": 3, "inputs": 8})
    );
    assert_eq!(report["levels"]["cyclomatic-complexity"], "warn");
    assert_eq!(report["levels"]["nested-ternary"], "off");
    assert_eq!(report["levels"]["long-comment"], "error");
    assert_eq!(report["files"], json!(["a.gd", "b.gd"]));
    let problem = json!({"path": "a.gd", "line": 4, "rule": "cognitive-complexity", "level": "error",
        "message": "busy has cognitive complexity 4 (max 2)"});
    assert_eq!(report["problems"], json!([problem]));
}

#[test]
fn json_problems_match_the_text_output() {
    let repo = json_repo();
    repo.write(
        "d.gd",
        "extends Node\n\n\nfunc f(a, b):\n\treturn 1 if a else (2 if b else 3)\n",
    );
    repo.write(
        ".godot-guard.yml",
        "limits:\n  cyclomatic: 1\nrules:\n  cyclomatic-complexity: warn\n",
    );
    let config = repo.config();
    let (status, report) = check_json(&config, None);
    let (text_status, text) = check(&config, None);
    assert_eq!(status, text_status);
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.starts_with("godot-guard check:"))
        .collect();
    let problems = report["problems"].as_array().unwrap();
    let printed: Vec<String> = problems
        .iter()
        .map(|p| {
            let prefix = if p["level"] == "warn" {
                "warning: "
            } else {
                ""
            };
            format!(
                "{}:{}: {prefix}{} ({})",
                p["path"].as_str().unwrap(),
                p["line"],
                p["message"].as_str().unwrap(),
                p["rule"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(printed, lines);
    assert!(problems.iter().any(|p| p["level"] == "warn"));
}

#[test]
fn json_warnings_alone_exit_zero() {
    let repo = json_repo();
    repo.write(
        ".godot-guard.yml",
        "limits:\n  cognitive: 2\nrules:\n  cognitive-complexity: warn\n",
    );
    let (status, report) = check_json(&repo.config(), Some(vec![repo.path("a.gd")]));
    assert_eq!(status, 0);
    let levels: Vec<&Value> = report["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| &p["level"])
        .collect();
    assert_eq!(levels, [&json!("warn")]);
}

#[test]
fn json_loose_and_stale_baseline_entries_are_problems() {
    let repo = json_repo();
    let config = repo.config();
    record(
        &config,
        "cognitive-complexity",
        &[("a.gd::busy", 5), ("a.gd::gone", 9), ("b.gd::gone", 9)],
    );
    let (status, report) = check_json(&config, Some(vec![repo.path("a.gd")]));
    assert_eq!(status, 1);
    assert_eq!(report["files"], json!(["a.gd"]));
    let found: Vec<(&str, i64, &str)> = report["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["path"].as_str().unwrap(),
                p["line"].as_i64().unwrap(),
                p["rule"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (".godot-guard-baseline.json", 4, "cognitive-complexity"),
            ("a.gd", 4, "cognitive-complexity")
        ]
    );
    assert!(
        report["problems"][1]["message"]
            .as_str()
            .unwrap()
            .contains("below its baseline 5")
    );
}

const NESTED: &str =
    "extends Node\n\n\nfunc f(a: bool) -> int:\n\treturn 1 if a else 2 if not a else 3\n";

#[test]
fn rule_off_silences_it() {
    let repo = Repo::new(&[("n.gd", NESTED)], Some("rules:\n  nested-ternary: off\n"));
    let (status, output) = check(&repo.config(), None);
    assert_eq!(status, 0);
    assert!(!output.contains("nested-ternary"));
}

#[test]
fn warn_level_prints_and_passes() {
    let repo = Repo::new(
        &[("n.gd", NESTED)],
        Some("limits:\n  cognitive: 1\nrules:\n  cognitive-complexity: warn\n"),
    );
    let (status, output) = check(&repo.config(), None);
    assert_eq!(status, 0);
    assert!(output.contains("n.gd:5: warning: nested conditional expression"));
    assert!(output.contains("warning: f has cognitive complexity"));
    assert!(output.contains("0 errors, 2 warnings, 1 file"));
}

fn weak_project(config_text: Option<&str>) -> Repo {
    Repo::new(
        &[
            ("scripts/a.gd", "extends Node\n"),
            ("project.godot", "config_version=5\n"),
        ],
        config_text,
    )
}

#[test]
fn a_full_check_fails_on_weak_settings() {
    let (status, output) = check(&weak_project(None).config(), None);
    assert_eq!(status, 1);
    assert!(output.contains("project.godot:1: "));
    assert!(output.contains("(analyzer-config)"));
}

#[test]
fn checking_named_files_leaves_project_godot_alone() {
    let repo = weak_project(None);
    let (status, output) = check(&repo.config(), Some(vec![repo.path("scripts/a.gd")]));
    assert_eq!((status, output.contains("analyzer-config")), (0, false));
}

#[test]
fn analyzer_warn_level_passes_and_off_is_silent() {
    let (status, output) = check(
        &weak_project(Some("rules:\n  analyzer-config: warn\n")).config(),
        None,
    );
    assert_eq!(status, 0);
    assert!(output.contains("warning: "));
    let (status, output) = check(
        &weak_project(Some("rules:\n  analyzer-config: off\n")).config(),
        None,
    );
    assert_eq!((status, output.contains("analyzer-config")), (0, false));
}

#[test]
fn json_reports_the_analyzer_problem() {
    let (_, report) = check_json(&weak_project(None).config(), None);
    let rules: Vec<&str> = report["problems"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["rule"].as_str().unwrap())
        .collect();
    assert_eq!(rules, ["analyzer-config"]);
}
