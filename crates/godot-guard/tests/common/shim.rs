//! A temporary repository with a fake Godot on PATH, for the gate tests.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use super::{Repo, Run};

pub const CLEAN: &str = "extends Node\n\nvar a := 1\n";

pub fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .envs([
            ("GIT_AUTHOR_NAME", "godot-guard tests"),
            ("GIT_AUTHOR_EMAIL", "tests@godot-guard.invalid"),
            ("GIT_COMMITTER_NAME", "godot-guard tests"),
            ("GIT_COMMITTER_EMAIL", "tests@godot-guard.invalid"),
        ])
        .output()
        .expect("git should run");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A temporary repository with a fake Godot that logs its arguments.
pub struct Shim {
    pub repo: Repo,
}

impl Shim {
    pub fn new() -> Shim {
        let shim = Shim {
            repo: Repo::new(
                &[("a.gd", CLEAN), ("b.gd", CLEAN), ("c.gd", CLEAN)],
                Some("paths: [.]\n"),
            ),
        };
        shim.godot(0, "");
        shim
    }

    pub fn godot(&self, status: i32, extra: &str) {
        let log = self.repo.root.join(".bin/calls.log");
        let text = format!(
            "#!/bin/sh\necho \"$(basename \"$0\") $*\" >> \"{}\"\n{extra}\nexit {status}\n",
            log.display()
        );
        let path = self.repo.write(".bin/godot", &text);
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.repo.root.join(".bin/calls.log"))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    pub fn run(&self, args: &[&str]) -> Run {
        self.run_in(&self.repo.root, args)
    }

    pub fn run_in(&self, directory: &Path, args: &[&str]) -> Run {
        let godot = self.repo.path(".bin/godot");
        self.repo.run_in(directory, args, &[("GODOT", &godot)])
    }

    pub fn stage(&self, name: &str, staged: &str, working: &str) {
        self.repo.write(name, staged);
        git(&self.repo.root, &["add", name]);
        self.repo.write(name, working);
    }

    pub fn init(&self) {
        git(&self.repo.root, &["init", "-q"]);
    }
}
