//! `godot-guard complexity --json`: the scores pedro-best-practices reads, in the bytes its Python
//! analyzer printed, for exactly the files given.

mod common;

use common::Repo;
use serde_json::{Value, json};

const NESTED: &str = "\
var hp := 0:
\tset(value):
\t\tif value > 0:
\t\t\thp = value

func outer(items):
\tvar pick = func(x): return x if x else 0
\tfor item in items:
\t\tif item and pick.call(item):
\t\t\tprint(item)
";

const SIMPLE: &str = "func ready():\n\tpass\n";

/// A repo without `.godot-guard.yml` or `project.godot`: complexity needs neither.
fn bare(files: &[(&str, &str)]) -> Repo {
    let repo = Repo::new(files, None);
    std::fs::remove_file(repo.root.join("project.godot")).unwrap();
    repo
}

#[test]
fn prints_each_function_compactly_without_a_trailing_newline() {
    let repo = bare(&[("a.gd", SIMPLE)]);
    let run = repo.run(&["complexity", "--json", "a.gd"]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(
        run.stdout,
        r#"{"a.gd": [{"name": "ready", "line": 1, "end_line": 1, "cyclomatic": 1, "cognitive": 0}]}"#
    );
}

#[test]
fn nested_functions_get_their_own_names_and_null_cognitive() {
    let repo = bare(&[("nested.gd", NESTED)]);
    let run = repo.run(&["complexity", "--json", "nested.gd"]);
    let report: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(
        report,
        json!({"nested.gd": [
            {"name": "hp.set", "line": 2, "end_line": 4, "cyclomatic": 2, "cognitive": 1},
            {"name": "outer", "line": 6, "end_line": 10, "cyclomatic": 4, "cognitive": 6},
            {"name": "outer.<lambda#1>", "line": 7, "end_line": 7, "cyclomatic": 2, "cognitive": null},
        ]})
    );
}

#[test]
fn keys_follow_the_arguments_as_given() {
    let repo = bare(&[("b.gd", SIMPLE), ("sub/a.gd", SIMPLE)]);
    let run = repo.run(&[
        "complexity",
        "--json",
        "./sub/a.gd",
        "b.gd",
        "sub/a.gd",
        "./sub/a.gd",
    ]);
    let report: serde_json::Map<String, Value> = serde_json::from_str(&run.stdout).unwrap();
    let keys: Vec<&str> = report.keys().map(String::as_str).collect();
    assert_eq!(keys, ["./sub/a.gd", "b.gd", "sub/a.gd"]);
}

#[test]
fn a_file_that_fails_gets_an_error_and_the_rest_still_print() {
    let repo = bare(&[("good.gd", SIMPLE), ("bad.gd", "func broken(:\n\tpass\n")]);
    let run = repo.run(&["complexity", "--json", "bad.gd", "missing.gd", "good.gd"]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    let report: Value = serde_json::from_str(&run.stdout).unwrap();
    let error = |path: &str| report[path]["error"].as_str().unwrap().to_owned();
    assert!(
        error("bad.gd").starts_with("Unexpected"),
        "{}",
        error("bad.gd")
    );
    assert!(
        error("missing.gd").starts_with("OSError: "),
        "{}",
        error("missing.gd")
    );
    assert_eq!(report["good.gd"][0]["name"], "ready");
}

#[test]
fn never_reads_the_config() {
    let repo = Repo::new(&[("skipped/a.gd", SIMPLE)], Some("not: [valid\n"));
    assert!(repo.run(&["check"]).status != 0);
    let run = repo.run(&["complexity", "--json", "skipped/a.gd"]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .starts_with(r#"{"skipped/a.gd": [{"name": "ready""#)
    );
}

#[test]
fn json_and_files_are_required() {
    let repo = bare(&[("a.gd", SIMPLE)]);
    for (args, missing) in [
        (&["complexity", "a.gd"][..], "--json"),
        (&["complexity", "--json"][..], "files"),
        (&["complexity"][..], "--json, files"),
    ] {
        let run = repo.run(args);
        assert_eq!((run.status, run.stdout.as_str()), (2, ""), "{args:?}");
        assert!(
            run.stderr.ends_with(&format!(
                "godot-guard complexity: error: the following arguments are required: {missing}\n"
            )),
            "{args:?}: {}",
            run.stderr
        );
    }
}
