//! `godot-guard init`: scaffold the config, hooks, and CI workflow.
//!
//! The templates are embedded at build time: the Python package's own hooks, and the crate's own
//! `godot-guard.yml`, which lists the keys added since 0.4.0, and `quality.yml`, which installs the release
//! binary where the Python package's installs the package with pip.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;

use crate::config::CONFIG_NAME;
use crate::error::{Failure, Result};

pub const TEMPLATES: [(&str, &str); 4] = [
    (
        "godot-guard.yml",
        include_str!("../templates/godot-guard.yml"),
    ),
    (
        "pre-commit",
        include_str!("../../../src/godot_guard/templates/pre-commit"),
    ),
    (
        "pre-push",
        include_str!("../../../src/godot_guard/templates/pre-push"),
    ),
    ("quality.yml", include_str!("../templates/quality.yml")),
];
const FALLBACK_GODOT: &str = "4.7.1";

static FEATURES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"config/features=PackedStringArray\("(\d+\.\d+(?:\.\d+)?)""#)
        .expect("a valid pattern")
});

/// The project's `config/features` version; a bare minor of the fallback keeps its patch.
fn godot_version(root: &Path) -> Result<String> {
    let project = root.join("project.godot");
    let text = if project.is_file() {
        fs::read_to_string(&project).map_err(|e| Failure::crash(project.display(), e))?
    } else {
        String::new()
    };
    Ok(match FEATURES.captures(&text) {
        Some(found) if !FALLBACK_GODOT.starts_with(&format!("{}.", &found[1])) => {
            found[1].to_owned()
        }
        _ => FALLBACK_GODOT.to_owned(),
    })
}

pub fn template(name: &str, values: &[(&str, &str)]) -> String {
    let (_, text) = TEMPLATES
        .iter()
        .find(|(file, _)| *file == name)
        .expect("a known template");
    values.iter().fold(text.to_string(), |text, (key, value)| {
        text.replace(&format!("{{{key}}}"), value)
    })
}

fn planned(root: &Path) -> Result<Vec<(&'static str, String)>> {
    let tests = if root.join("tests/run_tests.gd").is_file() {
        "[res://tests/run_tests.gd]"
    } else {
        "[]"
    };
    Ok(vec![
        (
            CONFIG_NAME,
            template("godot-guard.yml", &[("tests", tests)]),
        ),
        (".githooks/pre-commit", template("pre-commit", &[])),
        (".githooks/pre-push", template("pre-push", &[])),
        (
            ".github/workflows/quality.yml",
            template(
                "quality.yml",
                &[
                    ("godot_version", &godot_version(root)?),
                    ("version", env!("CARGO_PKG_VERSION")),
                ],
            ),
        ),
    ])
}

fn write_new(root: &Path, relative: &str, text: &str) -> Result<()> {
    let path = root.join(relative);
    if path.exists() {
        println!("skipped {relative}: already exists");
        return Ok(());
    }
    let failed = |e: std::io::Error| Failure::crash(path.display(), e);
    fs::create_dir_all(path.parent().expect("a file below the root")).map_err(failed)?;
    fs::write(&path, text).map_err(failed)?;
    if relative.starts_with(".githooks/") {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).map_err(failed)?;
    }
    println!("wrote {relative}");
    Ok(())
}

pub fn init(root: &Path) -> Result<i32> {
    for (relative, text) in planned(root)? {
        write_new(root, relative, &text)?;
    }
    let hooks = Command::new("git")
        .args(["config", "core.hooksPath", ".githooks"])
        .current_dir(root)
        .output()
        .map_err(|e| Failure::crash("git", e))?;
    if hooks.status.success() {
        println!("ran git config core.hooksPath .githooks");
    } else {
        println!("not a git repository: run `git config core.hooksPath .githooks` once it is");
    }
    Ok(0)
}
