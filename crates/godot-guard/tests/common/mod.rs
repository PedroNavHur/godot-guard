//! Temporary Godot repositories for the tests, as `tests/helpers.py` builds them.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use godot_guard::config::{self, Config};
use godot_guard::godot_warnings::{ERROR, REQUIRED};

/// A project.godot that passes analyzer-config: every required warning at error.
pub fn strict_project() -> String {
    let settings: String = REQUIRED
        .iter()
        .map(|(name, _)| format!("gdscript/warnings/{name}={ERROR}\n"))
        .collect();
    format!("config_version=5\n\n[debug]\n\n{settings}")
}

pub struct Repo {
    _temp: tempfile::TempDir,
    pub root: PathBuf,
}

impl Repo {
    pub fn new(files: &[(&str, &str)], config_text: Option<&str>) -> Repo {
        let temp = tempfile::tempdir().expect("a temporary directory");
        let root = fs::canonicalize(temp.path()).expect("a resolvable temporary directory");
        let repo = Repo { _temp: temp, root };
        repo.write("project.godot", &strict_project());
        if let Some(text) = config_text {
            repo.write(".godot-guard.yml", text);
        }
        for (name, text) in files {
            repo.write(name, text);
        }
        repo
    }

    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::create_dir_all(path.parent().expect("a parent")).expect("a parent directory");
        fs::write(&path, text).expect("a written file");
        path
    }

    pub fn config(&self) -> Config {
        config::load(&self.root).unwrap_or_else(|e| panic!("config should load: {e}"))
    }

    /// The config error's message.
    pub fn config_error(&self) -> String {
        match config::load(&self.root) {
            Ok(_) => panic!("expected a config error"),
            Err(error) => error.to_string(),
        }
    }

    pub fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }

    /// Run the godot-guard binary in the root.
    pub fn run(&self, args: &[&str]) -> Run {
        self.run_in(&self.root, args, &[])
    }

    pub fn run_in(&self, directory: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_godot-guard"))
            .args(args)
            .current_dir(directory)
            .envs(env.iter().copied())
            .output()
            .expect("godot-guard should run");
        Run {
            status: output.status.code().expect("an exit status"),
            stdout: String::from_utf8(output.stdout).expect("UTF-8 output"),
            stderr: String::from_utf8(output.stderr).expect("UTF-8 errors"),
        }
    }
}

#[derive(Debug)]
pub struct Run {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}
