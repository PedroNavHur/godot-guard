//! Temporary Godot repositories for the tests, as `tests/helpers.py` builds them.
#![allow(dead_code)]

pub mod shim;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use std::collections::BTreeMap;

use godot_guard::baseline::{self, Data};
use godot_guard::check::{self, Request};
use godot_guard::config::{self, Config};
use godot_guard::godot_warnings::{ERROR, REQUIRED};
use godot_guard::scripts::Scripts;
use serde_json::Value;

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

/// `check` in-process: its status and report.
pub fn check(config: &Config, files: Option<Vec<String>>) -> (i32, String) {
    let request = Request {
        files,
        ..Request::default()
    };
    check::check(config, request, &Scripts::new()).unwrap()
}

pub fn check_json(config: &Config, files: Option<Vec<String>>) -> (i32, Value) {
    let request = Request {
        files,
        as_json: true,
        ..Request::default()
    };
    let (status, text) = check::check(config, request, &Scripts::new()).unwrap();
    (status, serde_json::from_str(&text).unwrap())
}

pub fn update(config: &Config, init: bool, force: bool) -> (i32, String) {
    check::update_baseline(config, init, force, &Scripts::new()).unwrap()
}

/// A function with `count` sequential ifs: cognitive `count`.
pub fn branches(name: &str, count: usize) -> String {
    let body: String = (0..count)
        .map(|n| format!("\tif x == {n}:\n\t\treturn {n}\n"))
        .collect();
    format!("func {name}(x: int) -> int:\n{body}\treturn -1\n")
}

pub fn record(config: &Config, rule: &str, entries: &[(&str, i64)]) {
    let mut data = Data::empty();
    data.rule_mut(rule)
        .extend(entries.iter().map(|(key, value)| (key.to_string(), *value)));
    baseline::write(&config.baseline_path(), &data).unwrap();
}

pub fn recorded(config: &Config, rule: &str) -> BTreeMap<String, i64> {
    baseline::load(&config.baseline_path())
        .unwrap()
        .rule(rule)
        .clone()
}

pub fn entries(pairs: &[(&str, i64)]) -> BTreeMap<String, i64> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), *value))
        .collect()
}
