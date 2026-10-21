//! Port of tests/test_baseline.py.

mod common;

use common::{Repo, branches, check, entries, record, recorded, update};
use godot_guard::baseline::{self, Data};
use godot_guard::config::Config;
use godot_guard::rules::{self, Surfaces};
use godot_guard::scripts::Scripts;

const CONFIG: &str = "limits:\n  cognitive: 2\n  cyclomatic: 16\nrules:\n  nested-ternary: off\n";

struct Busy {
    repo: Repo,
    config: Config,
}

impl Busy {
    fn new() -> Busy {
        let source = format!(
            "extends Node\n\n\n{}\n\n{}",
            branches("busy", 4),
            branches("calm", 1)
        );
        let other = format!("extends Node\n\n\n{}", branches("other", 3));
        let repo = Repo::new(&[("a.gd", &source), ("b.gd", &other)], Some(CONFIG));
        let config = repo.config();
        Busy { repo, config }
    }

    fn record(&self, pairs: &[(&str, i64)]) {
        record(&self.config, "cognitive-complexity", pairs);
    }
}

#[test]
fn new_violation_is_an_error() {
    let (status, output) = check(&Busy::new().config, None);
    assert_eq!(status, 1);
    assert!(
        output.contains("a.gd:4: busy has cognitive complexity 4 (max 2) (cognitive-complexity)"),
        "{output}"
    );
    assert!(output.contains("2 errors, 0 warnings, 2 files"), "{output}");
}

#[test]
fn baselined_at_its_value_passes() {
    let busy = Busy::new();
    busy.record(&[("a.gd::busy", 4), ("b.gd::other", 3)]);
    assert_eq!(check(&busy.config, None).0, 0);
}

#[test]
fn worsened_loose_and_stale_entries_are_errors() {
    let busy = Busy::new();
    let cases: [(&[(&str, i64)], &str); 4] = [
        (
            &[("a.gd::busy", 3), ("b.gd::other", 3)],
            "busy has cognitive complexity 4 (baseline 3, max 2)",
        ),
        (
            &[("a.gd::busy", 6), ("b.gd::other", 3)],
            "below its baseline 6: lower it with `godot-guard baseline`",
        ),
        (
            &[("a.gd::busy", 4), ("a.gd::calm", 3), ("b.gd::other", 3)],
            "calm has cognitive complexity 1 is within the limit 2",
        ),
        (
            &[("a.gd::busy", 4), ("a.gd::gone", 5), ("b.gd::other", 3)],
            ".godot-guard-baseline.json:4: baseline entry 'a.gd::gone' no longer exists",
        ),
    ];
    for (pairs, expected) in cases {
        busy.record(pairs);
        let (status, output) = check(&busy.config, None);
        assert_eq!(status, 1);
        assert!(output.contains(expected), "{output}");
    }
}

#[test]
fn subset_only_judges_its_own_entries() {
    let busy = Busy::new();
    busy.record(&[("a.gd::busy", 4), ("b.gd::gone", 5), ("b.gd::other", 3)]);
    let (status, output) = check(&busy.config, Some(vec![busy.repo.path("a.gd")]));
    assert_eq!(status, 0, "{output}");
    assert!(output.contains("1 file"));
}

#[test]
fn baseline_tightens_and_never_adds() {
    let busy = Busy::new();
    busy.record(&[("a.gd::busy", 6), ("a.gd::calm", 3), ("a.gd::gone", 5)]);
    let (status, output) = update(&busy.config, false, false);
    assert_eq!(status, 0);
    assert_eq!(
        recorded(&busy.config, "cognitive-complexity"),
        entries(&[("a.gd::busy", 4)])
    );
    assert!(output.contains("lowered cognitive-complexity a.gd::busy: 6 -> 4"));
    assert!(output.contains("removed cognitive-complexity a.gd::gone: no longer exists"));
}

#[test]
fn init_records_every_violation() {
    let busy = Busy::new();
    let (status, output) = update(&busy.config, true, false);
    assert_eq!(status, 0);
    assert_eq!(
        recorded(&busy.config, "cognitive-complexity"),
        entries(&[("a.gd::busy", 4), ("b.gd::other", 3)])
    );
    assert!(output.contains("cognitive-complexity 2"));
    let text = std::fs::read_to_string(busy.config.baseline_path()).unwrap();
    assert!(text.ends_with("}\n"));
    assert!(text.contains("  \"version\": 1"));
}

#[test]
fn init_refuses_to_overwrite() {
    let busy = Busy::new();
    busy.record(&[]);
    let (status, output) = update(&busy.config, true, false);
    assert_eq!(status, 1);
    assert!(output.contains("--force"));
    assert_eq!(update(&busy.config, true, true).0, 0);
    assert_eq!(recorded(&busy.config, "cognitive-complexity").len(), 2);
}

#[test]
fn unparsable_file_keeps_its_function_entries() {
    let busy = Busy::new();
    busy.record(&[("a.gd::busy", 4), ("b.gd::other", 3)]);
    busy.repo.write("b.gd", "func other(:\n");
    let (_, output) = check(&busy.config, None);
    assert!(output.contains("(parse-error)"));
    assert!(!output.contains("no longer exists"));
    update(&busy.config, false, false);
    assert_eq!(
        recorded(&busy.config, "cognitive-complexity"),
        entries(&[("a.gd::busy", 4), ("b.gd::other", 3)])
    );
}

#[test]
fn long_comment_count_is_baselined_per_file() {
    let comment = "# one\n# two\n# three\n";
    let source = format!("extends Node\nvar a := 1\n{comment}var b := 1\n{comment}var c := 1\n");
    let repo = Repo::new(&[("c.gd", &source)], None);
    let config = repo.config();
    assert_eq!(check(&config, None).1.matches("(long-comment)").count(), 2);
    update(&config, true, false);
    assert_eq!(check(&config, None).0, 0);
    repo.write("c.gd", &format!("{source}{comment}var d := 1\n"));
    let (status, output) = check(&config, None);
    assert_eq!(status, 1);
    assert!(
        output.contains("the file has 3 long comments, its baseline allows 2"),
        "{output}"
    );
}

#[test]
fn a_long_file_is_not_a_finding() {
    let body: String = (0..500).map(|n| format!("var v{n} := {n}\n")).collect();
    let repo = Repo::new(&[("long.gd", &format!("extends Node\n{body}"))], None);
    assert_eq!(check(&repo.config(), None).0, 0);
}

fn lambda(n: usize) -> String {
    let body: String = (0..n)
        .map(|k| format!("\t\tif x == {k}:\n\t\t\treturn {k}\n"))
        .collect();
    format!("{body}\t\treturn -1\n")
}

#[test]
fn each_lambda_is_judged_on_its_own() {
    let source = format!(
        "extends Node\n\n\nfunc f():\n\tvar old = func(x):\n{}\tvar new = func(x):\n{}\treturn [old, new]\n",
        lambda(4),
        lambda(3)
    );
    let repo = Repo::new(
        &[("a.gd", &source)],
        Some("limits:\n  cognitive: 100\n  cyclomatic: 2\n"),
    );
    let config = repo.config();
    record(
        &config,
        "cyclomatic-complexity",
        &[("a.gd::f.<lambda#1>", 5)],
    );
    let (status, output) = check(&config, None);
    assert_eq!(status, 1);
    assert!(
        output.contains("a.gd:15: f.<lambda#2> has cyclomatic complexity 4 (max 2)"),
        "{output}"
    );
    assert!(!output.contains("<lambda#1>"));
    assert_eq!(update(&repo.config(), true, true).0, 0);
    let expected = entries(&[("a.gd::f.<lambda#1>", 5), ("a.gd::f.<lambda#2>", 4)]);
    assert_eq!(recorded(&config, "cyclomatic-complexity"), expected);
}

#[test]
fn repeated_names_are_numbered() {
    let repo = Repo::new(
        &[(
            "a.gd",
            "extends Node\n\n\nfunc f():\n\tpass\n\n\nfunc f():\n\tpass\n",
        )],
        None,
    );
    let (config, scripts) = (repo.config(), Scripts::new());
    let mut surfaces = Surfaces::new(&scripts, &repo.root, None);
    let results = rules::analyze(&config, &["a.gd".to_owned()], &mut surfaces, &scripts).unwrap();
    let keys: Vec<&str> = results[0]
        .items
        .iter()
        .filter(|i| i.rule == "cyclomatic-complexity")
        .map(|i| i.key.as_str())
        .collect();
    assert_eq!(keys, ["a.gd::f#1", "a.gd::f#2"]);
}

#[test]
fn paths_holding_the_key_separator() {
    assert_eq!(
        baseline::item_path("cognitive-complexity", "a::b.gd::Inner.work"),
        "a::b.gd"
    );
    assert_eq!(baseline::item_path("long-comment", "a::b.gd"), "a::b.gd");
}

#[test]
fn subset_check_reports_a_missing_function_in_such_a_file() {
    let repo = Repo::new(
        &[("a::b.gd", "extends Resource\n\n\nfunc calm():\n\tpass\n")],
        Some(CONFIG),
    );
    let config = repo.config();
    let mut data = Data::empty();
    data.rule_mut("cognitive-complexity")
        .insert("a::b.gd::work".into(), 3);
    data.rule_mut("interface-cap").insert("a::b.gd".into(), 9);
    baseline::write(&config.baseline_path(), &data).unwrap();
    let (status, output) = check(&config, Some(vec![repo.path("a::b.gd")]));
    assert_eq!(status, 1);
    assert!(output.contains("baseline entry 'a::b.gd::work' no longer exists"));
    assert!(output.contains("baseline entry 'a::b.gd' no longer exists"));
}
