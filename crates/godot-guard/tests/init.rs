//! Port of tests/test_init.py: `init`'s scaffolding, and the hook templates under a fake host-check.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use common::Repo;
use godot_guard::config::TestScript;
use godot_guard::scaffold::{REQUIREMENT, TEMPLATES};

fn init_repo() -> Repo {
    let repo = Repo::new(
        &[
            ("tests/run_tests.gd", "extends SceneTree\n"),
            ("tools/requirements-lint.txt", "gdtoolkit==4.5.0\n"),
        ],
        None,
    );
    repo.write(
        "project.godot",
        "config/features=PackedStringArray(\"4.7\", \"GL Compatibility\")\n",
    );
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&repo.root)
            .status()
            .unwrap()
            .success()
    );
    repo
}

fn read(root: &Path, name: &str) -> String {
    fs::read_to_string(root.join(name)).unwrap()
}

#[test]
fn scaffolds_everything_once() {
    let repo = init_repo();
    let run = repo.run(&["init"]);
    assert_eq!(run.status, 0, "{run:?}");
    let root = &repo.root;
    assert_eq!(
        repo.config().gate.tests,
        [TestScript {
            script: "res://tests/run_tests.gd".into(),
            shards: None
        }]
    );
    let hook = root.join(".githooks/pre-commit");
    assert_ne!(fs::metadata(&hook).unwrap().permissions().mode() & 0o111, 0);
    assert!(read(root, ".githooks/pre-commit").contains("gate --fast --staged"));
    assert!(read(root, ".github/workflows/quality.yml").contains("version: 4.7.1"));
    assert_eq!(
        read(root, "tools/requirements-lint.txt"),
        format!("gdtoolkit==4.5.0\n{REQUIREMENT}\n")
    );
    let hooks = Command::new("git")
        .args(["config", "core.hooksPath"])
        .current_dir(root)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&hooks.stdout).trim(), ".githooks");
    assert!(
        run.stdout
            .contains("ran git config core.hooksPath .githooks"),
        "{run:?}"
    );
    let again = repo.run(&["init"]);
    assert_eq!(again.stdout.matches("skipped").count(), 5, "{again:?}");
}

#[test]
fn newer_minor_version_is_used_as_is() {
    let repo = init_repo();
    repo.write(
        "project.godot",
        "config/features=PackedStringArray(\"4.8\", \"Forward Plus\")\n",
    );
    assert_eq!(repo.run(&["init"]).status, 0);
    assert!(read(&repo.root, ".github/workflows/quality.yml").contains("version: 4.8\n"));
}

const HOST_CHECK: &str = r#"#!/bin/sh
if [ "${HOST_CHECK_ACTIVE:-}" = 1 ]; then echo nested >> "$LOG"; exit 64; fi
echo "host-check $*" >> "$LOG"
HOST_CHECK_ACTIVE=1 exec "$@"
"#;
const GUARD: &str = "#!/bin/sh\necho \"godot-guard $*\" >> \"$LOG\"\n";

/// Run the `name` hook template under a fake host-check that refuses nesting, as the real one does.
fn hook(name: &str, active: Option<&str>) -> (i32, Vec<String>) {
    let temp = tempfile::tempdir().unwrap();
    for (tool, text) in [("host-check", HOST_CHECK), ("godot-guard", GUARD)] {
        let path = temp.path().join(tool);
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let script = temp.path().join(format!("{name}.sh"));
    let template = TEMPLATES.iter().find(|(file, _)| *file == name).unwrap().1;
    fs::write(&script, template).unwrap();
    let log = temp.path().join("calls.log");
    let path = format!(
        "{}:{}",
        temp.path().display(),
        std::env::var("PATH").unwrap()
    );
    let mut command = Command::new("sh");
    command
        .arg(&script)
        .current_dir(temp.path())
        .env("PATH", path)
        .env("LOG", &log)
        .env_remove("HOST_CHECK_ACTIVE");
    if let Some(active) = active {
        command.env("HOST_CHECK_ACTIVE", active);
    }
    let status = command.status().unwrap().code().unwrap();
    (
        status,
        fs::read_to_string(&log)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}

#[test]
fn hooks_queue_through_host_check() {
    let expected = [
        "host-check godot-guard gate --fast --staged",
        "godot-guard gate --fast --staged",
    ];
    assert_eq!(
        hook("pre-commit", None),
        (0, expected.map(String::from).to_vec())
    );
}

#[test]
fn hooks_inside_a_host_check_job_run_directly() {
    assert_eq!(
        hook("pre-push", Some("1")),
        (0, vec!["godot-guard gate".to_owned()])
    );
}
