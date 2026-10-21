//! Port of tests/test_gate.py's full-gate, directory, and run tests, through the binary with a fake
//! Godot on PATH; its staged-snapshot tests are in tests/staged.rs.
//!
//! gdformat and gdlint run in-process now, so where 0.4.0's tests read their shims' call log these read
//! the tools' own output. The parser warm-up tests have no counterpart: there is no gdtoolkit process.

mod common;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use common::Repo;
use common::shim::{CLEAN, Shim, git};
use regex::Regex;

#[test]
fn runs_every_step_in_order() {
    let shim = Shim::new();
    shim.repo.write(
        ".godot-guard.yml",
        "paths: [scripts, .]\nexclude: [addons/**]\n\
         gate:\n  steps: ['echo step >> .bin/calls.log']\n  tests: [res://tests/run_tests.gd]\n",
    );
    let run = shim.run(&["gate"]);
    assert_eq!(run.status, 0, "{run:?}");
    let root = shim.repo.root.display();
    let calls = shim.calls();
    assert_eq!(calls.len(), 5, "{calls:?}");
    assert_eq!(calls[0], "step");
    assert_eq!(
        calls[1],
        format!("godot --headless --path {root} --editor --quit")
    );
    let analyzer = Regex::new(&format!(
        r"^godot --headless --path {} --script (/\S+/analyze_scripts\.gd) -- --root res://scripts --root res:// --exclude addons/\*\*$",
        regex::escape(&root.to_string())
    ))
    .unwrap();
    let script = analyzer
        .captures(&calls[2])
        .unwrap_or_else(|| panic!("{}", calls[2]))[1]
        .to_owned();
    assert!(
        !Path::new(&script).exists(),
        "the analyzer copy outlived the gate"
    );
    assert_eq!(
        calls[3],
        format!("godot --headless --path {root} --script res://tests/run_tests.gd")
    );
    assert_eq!(
        calls[4],
        format!("godot --headless --path {root} --quit-after 3")
    );
}

#[test]
fn godot_warning_fails_the_gate() {
    let shim = Shim::new();
    shim.godot(0, "echo 'WARNING: something odd'");
    let run = shim.run(&["gate"]);
    assert_eq!(run.status, 1);
    assert!(
        run.stdout
            .contains("WARNING: something odd\n== import failed"),
        "{run:?}"
    );
}

#[test]
fn a_captured_godot_step_keeps_its_output_and_warnings_still_fail() {
    let shim = Shim::new();
    shim.godot(0, "echo 'WARNING: something odd'");
    let run = shim.run(&["gate", "--jobs", "3"]);
    assert_eq!(run.status, 1);
    assert!(
        run.stdout
            .contains("== import\nWARNING: something odd\n== import failed"),
        "{run:?}"
    );
    assert!(run.stdout.ends_with("== failed: import\n"), "{run:?}");
}

const GODOT_ONLY: &str =
    "gate:\n  format: false\n  lint: false\n  import: false\n  analyzer: false\n  smoke: false\n";

#[test]
fn sharded_entries_run_one_process_per_shard() {
    let shim = Shim::new();
    shim.repo.write(
        ".godot-guard.yml",
        &format!("{GODOT_ONLY}  tests: [{{script: res://tests/run_tests.gd, shards: 2}}]\n"),
    );
    let run = shim.run(&["gate"]);
    assert_eq!(run.status, 0, "{run:?}");
    let root = shim.repo.root.display();
    assert_eq!(
        shim.calls(),
        [1, 2].map(|i| format!(
            "godot --headless --path {root} --script res://tests/run_tests.gd -- --shard={i}/2"
        ))
    );
    assert!(
        run.stdout
            .contains("== tests res://tests/run_tests.gd [2/2]"),
        "{run:?}"
    );
}

#[test]
fn a_mapping_with_one_shard_passes_the_argument_and_a_string_does_not() {
    let shim = Shim::new();
    let tests = "  tests: [res://tests/plain.gd, {script: res://tests/one.gd, shards: 1}]\n";
    shim.repo
        .write(".godot-guard.yml", &format!("{GODOT_ONLY}{tests}"));
    let run = shim.run(&["gate"]);
    assert_eq!(run.status, 0, "{run:?}");
    let root = shim.repo.root.display();
    assert_eq!(
        shim.calls(),
        [
            format!("godot --headless --path {root} --script res://tests/plain.gd"),
            format!("godot --headless --path {root} --script res://tests/one.gd -- --shard=1/1"),
        ]
    );
    assert!(
        run.stdout.contains("== tests res://tests/one.gd [1/1]"),
        "{run:?}"
    );
}

fn directories() -> Shim {
    let shim = Shim::new();
    for name in [
        "scripts/d.gd",
        "scripts/gen/e.gd",
        "scripts/.hidden/f.gd",
        "scripts/notes.txt",
    ] {
        shim.repo.write(name, CLEAN);
    }
    shim.repo
        .write(".godot-guard.yml", "paths: [.]\nexclude: [scripts/gen]\n");
    shim
}

#[test]
fn format_expands_directories() {
    let shim = directories();
    shim.repo.write("scripts/d.gd", "extends Node\nvar a=1\n");
    let run = shim.run(&[
        "format",
        &shim.repo.path("scripts"),
        &shim.repo.path("a.gd"),
    ]);
    assert_eq!(run.status, 0, "{run:?}");
    assert_eq!(
        run.stdout,
        "reformatted scripts/d.gd\n1 file reformatted, 1 file left unchanged.\n"
    );
}

#[test]
fn fast_gate_expands_directories() {
    let shim = directories();
    let run = shim.run(&["gate", "--fast", &shim.repo.path("scripts")]);
    assert_eq!(run.status, 0, "{run:?}");
    assert!(
        run.stdout
            .contains("== format\n1 file would be left unchanged\n"),
        "{run:?}"
    );
    assert!(
        run.stdout.contains("0 errors, 0 warnings, 1 file\n"),
        "{run:?}"
    );
}

#[test]
fn hidden_or_excluded_arguments_select_nothing() {
    let shim = directories();
    let names = [
        "scripts/.hidden",
        "scripts/.hidden/f.gd",
        "scripts/gen",
        "scripts/gen/e.gd",
    ];
    let paths: Vec<String> = names.iter().map(|name| shim.repo.path(name)).collect();
    let mut args = vec!["format"];
    args.extend(paths.iter().map(String::as_str));
    let run = shim.run(&args);
    assert_eq!(run.status, 0, "{run:?}");
    assert_eq!(
        run.stdout,
        "godot-guard format: no GDScript files selected\n"
    );
    for name in names {
        assert!(run.stderr.contains(&format!("skipping {name}:")), "{run:?}");
    }
    assert!(
        run.stderr
            .contains("scripts/.hidden: hidden directories are not scanned"),
        "{run:?}"
    );
    assert!(
        run.stderr.contains("scripts/gen: it matches exclude"),
        "{run:?}"
    );
}

#[test]
fn links_in_hidden_or_excluded_directories_select_nothing() {
    let shim = directories();
    for (name, target) in [
        (".hidden/alias.gd", "../a.gd"),
        (".hidden/directory", "../scripts"),
        ("scripts/gen/alias.gd", "../d.gd"),
    ] {
        let path = shim.repo.root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(target, path).unwrap();
    }
    let names = [
        ".hidden/alias.gd",
        ".hidden/directory",
        ".hidden/directory/d.gd",
        "scripts/gen/alias.gd",
    ];
    let paths: Vec<String> = names.iter().map(|name| shim.repo.path(name)).collect();
    let mut args = vec!["format"];
    args.extend(paths.iter().map(String::as_str));
    let run = shim.run(&args);
    assert_eq!(run.status, 0, "{run:?}");
    assert_eq!(
        run.stdout,
        "godot-guard format: no GDScript files selected\n"
    );
    for name in names {
        assert!(run.stderr.contains(&format!("skipping {name}:")), "{run:?}");
    }
}

#[test]
fn staged_scripts_in_hidden_directories_are_not_selected() {
    let shim = directories();
    shim.init();
    git(&shim.repo.root, &["add", "scripts/.hidden/f.gd"]);
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 0, "{run:?}");
    assert!(run.stdout.contains("no GDScript files selected"), "{run:?}");
}

#[test]
fn staged_and_files_together_are_refused() {
    let shim = Shim::new();
    let run = shim.run(&["gate", "--staged", "a.gd"]);
    assert_eq!(
        (run.status, run.stderr.as_str()),
        (1, "godot-guard gate: pass --staged or files, not both\n")
    );
}

#[test]
fn run_fails_on_diagnostics_and_keeps_the_exit_status() {
    let shim = Shim::new();
    for (extra, status, expected) in [
        ("echo 'WARNING: unused variable'", 0, 1),
        ("echo '  SCRIPT ERROR: Parse Error'", 0, 1),
        ("echo 'Godot Engine v4.7.1'", 0, 0),
        ("echo bye", 3, 3),
    ] {
        shim.godot(status, extra);
        let run = shim.run(&["run", "--", "--headless", "--version"]);
        assert_eq!(run.status, expected, "{extra}: {run:?}");
        assert!(run.stdout.ends_with(&format!(
            "{}\n",
            extra.trim_start_matches("echo ").trim_matches('\'')
        )));
    }
    assert_eq!(shim.calls().last().unwrap(), "godot --headless --version");
}

#[test]
fn a_missing_godot_fails_the_step_with_127() {
    let shim = Shim::new();
    shim.repo.write(
        ".godot-guard.yml",
        "gate:\n  format: false\n  lint: false\n  analyzer: false\n  smoke: false\n",
    );
    if Path::new(godot_guard::tools::MAC_GODOT).exists() {
        return; // the app bundle is the last fallback; with it installed, Godot is never missing
    }
    let run = shim.repo.run_in(
        &shim.repo.root,
        &["gate"],
        &[("GODOT", ""), ("PATH", "/nonexistent")],
    );
    assert_eq!(run.status, 1, "{run:?}");
    assert!(
        run.stdout.contains(
            "== import\ngodot-guard: Godot 4 not found: set $GODOT or put godot on PATH\n"
        )
    );
}

/// The embedded analyzer script under a real headless Godot; set GG_REAL_GODOT to run it.
#[test]
fn the_analyzer_under_a_real_godot() {
    if std::env::var_os("GG_REAL_GODOT").is_none() {
        eprintln!("skipped: set GG_REAL_GODOT to run the analyzer under Godot");
        return;
    }
    let repo = Repo::new(
        &[
            ("a.gd", CLEAN),
            ("scripts/b.gd", CLEAN),
            ("empty/notes.txt", ""),
        ],
        None,
    );
    let analyzer = repo.write(".analyzer/analyze_scripts.gd", godot_guard::gate::ANALYZER);
    let analyze = |roots: &[&str]| {
        let mut args = vec![
            "run".to_owned(),
            "--headless".into(),
            "--path".into(),
            repo.path("."),
        ];
        args.extend([
            "--script".into(),
            analyzer.to_string_lossy().into_owned(),
            "--".into(),
        ]);
        args.extend(
            roots
                .iter()
                .flat_map(|root| ["--root".to_owned(), root.to_string()]),
        );
        repo.run(&args.iter().map(String::as_str).collect::<Vec<_>>())
    };
    let run = analyze(&["res://a.gd"]);
    assert_eq!(run.status, 0, "{run:?}");
    assert!(
        run.stdout.contains("Analyzed 1 scripts, 0 failed to load."),
        "{run:?}"
    );
    let run = analyze(&["res://missing", "res://a.gd"]);
    assert_eq!(run.status, 1, "{run:?}");
    assert!(
        run.stdout
            .contains("ANALYZER: root res://missing does not exist"),
        "{run:?}"
    );
    let run = analyze(&["res://empty"]);
    assert_eq!(run.status, 1, "{run:?}");
    assert!(
        run.stdout
            .contains("ANALYZER: no scripts under res://empty"),
        "{run:?}"
    );
}
