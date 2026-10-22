//! Port of tests/test_gate.py's fast staged gate tests: the gate checks the index's content, not the
//! working tree's.

mod common;

use std::fs;
use std::os::unix::fs::symlink;

use common::Run;
use common::shim::{CLEAN, Shim, git};

const RESOURCE_BASE: &str = "class_name Base\nextends Resource\n";
const NODE_BASE: &str = "class_name Base\nextends Node\n";

fn wide() -> String {
    let exports: String = (0..9).map(|n| format!("@export var v{n} := 1\n")).collect();
    format!("extends Base\n\n{exports}")
}

fn fast_gate() -> Shim {
    let shim = Shim::new();
    shim.init();
    git(&shim.repo.root, &["add", "a.gd", "b.gd"]);
    fs::remove_file(shim.repo.root.join("b.gd")).unwrap();
    shim
}

#[test]
fn staged_files_are_checked_even_when_deleted_from_the_working_tree() {
    let shim = fast_gate();
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 0, "{run:?}");
    assert!(
        run.stdout
            .contains("== format\n2 files would be left unchanged\n"),
        "{run:?}"
    );
    assert!(
        run.stdout.contains("== lint\nSuccess: no problems found\n"),
        "{run:?}"
    );
    assert!(
        run.stdout.contains("0 errors, 0 warnings, 2 files"),
        "{run:?}"
    );
    assert!(run.stdout.contains("== check passed in"), "{run:?}");
}

#[test]
fn nothing_staged_is_nothing_to_do() {
    let shim = fast_gate();
    git(&shim.repo.root, &["rm", "-q", "--cached", "a.gd", "b.gd"]);
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 0);
    assert_eq!(run.stdout, "godot-guard gate: no GDScript files selected\n");
}

#[test]
fn failing_format_stops_the_gate() {
    let shim = fast_gate();
    shim.stage("a.gd", "extends Node\nvar a=1\n", CLEAN);
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 1);
    // Serial steps write to the terminal, so gdformat's report is on stderr.
    assert!(run.stderr.contains("would reformat a.gd"), "{run:?}");
    assert!(run.stdout.contains("== format failed"), "{run:?}");
    assert!(!run.stdout.contains("== lint"), "{run:?}");
}

fn staged_content() -> Shim {
    let shim = Shim::new();
    shim.init();
    shim
}

#[test]
fn invalid_staged_content_fails_with_a_clean_working_copy() {
    let shim = staged_content();
    shim.stage("a.gd", "func f(:\n", CLEAN);
    let run = shim.run(&["gate", "--fast", "--staged", "--jobs", "3"]);
    assert_eq!(run.status, 1);
    assert!(run.stdout.contains("== format failed"), "{run:?}");
    assert!(run.stdout.contains("== lint failed"), "{run:?}");
    assert!(run.stdout.contains("a.gd:1: cannot parse"), "{run:?}");
}

#[test]
fn lint_reads_the_tool_configs_copied_into_the_snapshot() {
    let shim = staged_content();
    shim.repo.write(".gdlintrc", "max-line-length: 10\n");
    shim.stage("a.gd", CLEAN, CLEAN);
    let run = shim.run(&["gate", "--fast", "--staged", "--jobs", "3"]);
    assert!(run.stdout.contains("(max-line-length)"), "{run:?}");
}

#[test]
fn valid_staged_content_passes_with_a_broken_working_copy() {
    let shim = staged_content();
    shim.stage("a.gd", CLEAN, "func f(:\n");
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 0, "{run:?}");
}

#[test]
fn resource_exclusion_reads_the_index_for_selected_scripts() {
    let shim = staged_content();
    shim.stage("base.gd", NODE_BASE, RESOURCE_BASE);
    shim.stage("wide.gd", &wide(), &wide());
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 1);
    assert!(
        run.stdout
            .contains("wide.gd:1: 9 inputs (@export 9) (max 8) (interface-cap)"),
        "{run:?}"
    );
}

/// Commit z_old.gd declaring `Base extends Resource` and an `extends Base` script, restage, and gate.
fn restage_base(delete_old: bool, add_new: bool) -> Run {
    let shim = staged_content();
    shim.stage("z_old.gd", RESOURCE_BASE, RESOURCE_BASE);
    shim.stage("wide.gd", &wide(), &wide());
    git(&shim.repo.root, &["commit", "-qm", "base"]);
    if delete_old {
        git(&shim.repo.root, &["rm", "-q", "--cached", "z_old.gd"]);
    }
    if add_new {
        shim.stage("a_new.gd", NODE_BASE, NODE_BASE);
    }
    let restaged = wide() + "\n";
    shim.stage("wide.gd", &restaged, &restaged);
    shim.run(&["gate", "--fast", "--staged", "--jobs", "3"])
}

#[test]
fn staged_class_names_and_deletions_shape_the_resource_map() {
    for (delete_old, add_new) in [(false, true), (true, false), (true, true)] {
        let run = restage_base(delete_old, add_new);
        assert_eq!(run.status, 1, "{delete_old} {add_new}: {run:?}");
        assert!(
            run.stdout.contains("wide.gd:1: 9 inputs"),
            "{delete_old} {add_new}: {run:?}"
        );
    }
}

#[test]
fn resource_exclusion_reads_the_working_tree_for_unselected_scripts() {
    let shim = staged_content();
    let source = shim.repo.root.join(".source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("base.gd"), RESOURCE_BASE).unwrap();
    git(&source, &["init", "-q"]);
    git(&source, &["add", "base.gd"]);
    git(&source, &["commit", "-qm", "base"]);
    let source = source.to_string_lossy().into_owned();
    git(
        &shim.repo.root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            &source,
            "vendor",
        ],
    );
    shim.stage("wide.gd", &wide(), &wide());
    let run = shim.run(&["gate", "--fast", "--staged", "--jobs", "3"]);
    assert!(
        run.stdout.contains("0 errors, 0 warnings, 1 file\n"),
        "{run:?}"
    );
}

#[test]
fn unchanged_symlink_to_a_non_script_is_ignored() {
    let shim = staged_content();
    shim.repo.write("target.txt", CLEAN);
    symlink("target.txt", shim.repo.root.join("alias.gd")).unwrap();
    git(&shim.repo.root, &["add", "target.txt", "alias.gd"]);
    git(&shim.repo.root, &["commit", "-qm", "alias"]);
    shim.stage("a.gd", CLEAN, CLEAN);
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 0, "{run:?}");
    assert!(
        run.stdout.contains("0 errors, 0 warnings, 1 file\n"),
        "{run:?}"
    );
}

#[test]
fn staged_symlinked_scripts_fail_the_gate() {
    let shim = staged_content();
    shim.stage("target.gd", CLEAN, CLEAN);
    shim.stage("retyped.gd", CLEAN, CLEAN);
    git(&shim.repo.root, &["commit", "-qm", "scripts"]);
    fs::remove_file(shim.repo.root.join("retyped.gd")).unwrap();
    for name in ["alias.gd", "retyped.gd"] {
        symlink("target.gd", shim.repo.root.join(name)).unwrap();
    }
    git(&shim.repo.root, &["add", "alias.gd", "retyped.gd"]);
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 1, "{run:?}");
    let message = ": symlinked scripts are not supported; commit the real file\n";
    assert_eq!(run.stdout, format!("alias.gd{message}retyped.gd{message}"));
}

#[test]
fn baseline_comes_from_the_index() {
    let shim = staged_content();
    let node = wide().replace("Base", "Node");
    shim.stage("wide.gd", &node, &node);
    shim.stage(
        ".godot-guard-baseline.json",
        "{\"interface-cap\": {\"wide.gd\": 9}}\n",
        "{\"interface-cap\": {}}\n",
    );
    let run = shim.run(&["gate", "--fast", "--staged"]);
    assert_eq!(run.status, 0, "{run:?}");
}

#[test]
fn config_root_below_the_git_root() {
    let shim = staged_content();
    shim.repo.write("game/project.godot", "config_version=5\n");
    shim.repo.write("game/.godot-guard.yml", "paths: [.]\n");
    shim.stage("game/scripts/a.gd", "func f(:\n", CLEAN);
    shim.stage("other.gd", "func f(:\n", CLEAN);
    let run = shim.run_in(
        &shim.repo.root.join("game"),
        &["gate", "--fast", "--staged", "--jobs", "3"],
    );
    assert_eq!(run.status, 1);
    assert!(
        run.stdout.contains("scripts/a.gd:1: cannot parse"),
        "{run:?}"
    );
    assert!(run.stdout.contains(", 1 file\n"), "{run:?}");
    assert!(!run.stdout.contains("other.gd"), "{run:?}");
}
