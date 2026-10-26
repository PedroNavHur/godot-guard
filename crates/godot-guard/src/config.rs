//! Load and validate `.godot-guard.yml`, and select the scripts it scans.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Failure, Result};
use crate::fnmatch::Pattern;
use crate::settings;
use pycompat::builtins;

pub const CONFIG_NAME: &str = ".godot-guard.yml";
pub const BASELINE_NAME: &str = ".godot-guard-baseline.json";
pub const AUTO: &str = "auto";

/// A rule's level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Off,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Off => "off",
        }
    }
}

/// `limits`, in 0.4.0's key order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limits {
    pub cognitive: i64,
    pub cyclomatic: i64,
    pub comment_lines: i64,
    pub inputs: i64,
}

/// One of the `limits` keys: the only place a key name maps to a `Limits` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Cognitive,
    Cyclomatic,
    CommentLines,
    Inputs,
}

impl Limit {
    /// In 0.4.0's key order.
    pub const ALL: [Limit; 4] = [
        Limit::Cognitive,
        Limit::Cyclomatic,
        Limit::CommentLines,
        Limit::Inputs,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Limit::Cognitive => "cognitive",
            Limit::Cyclomatic => "cyclomatic",
            Limit::CommentLines => "comment-lines",
            Limit::Inputs => "inputs",
        }
    }

    pub fn names() -> [&'static str; 4] {
        Self::ALL.map(Limit::name)
    }
}

impl Limits {
    pub fn get(&self, limit: Limit) -> i64 {
        match limit {
            Limit::Cognitive => self.cognitive,
            Limit::Cyclomatic => self.cyclomatic,
            Limit::CommentLines => self.comment_lines,
            Limit::Inputs => self.inputs,
        }
    }

    pub fn set(&mut self, limit: Limit, value: i64) {
        match limit {
            Limit::Cognitive => self.cognitive = value,
            Limit::Cyclomatic => self.cyclomatic = value,
            Limit::CommentLines => self.comment_lines = value,
            Limit::Inputs => self.inputs = value,
        }
    }

    pub fn entries(&self) -> [(&'static str, i64); 4] {
        Limit::ALL.map(|limit| (limit.name(), self.get(limit)))
    }
}

/// `gate.jobs`: a worker count, or `auto` for this process's CPU budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Jobs {
    Count(usize),
    Auto,
}

/// A `gate.tests` entry: a res:// script. `shards` None is a plain entry; a count runs that many Godot
/// processes, numbered 1 to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestScript {
    pub script: String,
    pub shards: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Gate {
    pub format: bool,
    pub lint: bool,
    pub steps: Vec<String>,
    pub import: bool,
    pub analyzer: bool,
    pub tests: Vec<TestScript>,
    /// Shell commands run from the root in the TEST phase, after the tests and before smoke.
    pub test_commands: Vec<String>,
    /// Godot's arguments for the smoke run; empty (`false` or `[]`) skips it.
    pub smoke: Vec<String>,
    pub jobs: Jobs,
}

#[derive(Clone, Debug)]
pub struct Boundary {
    pub name: String,
    pub paths: Vec<Pattern>,
    pub forbid: Vec<fancy_regex::Regex>,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub root: PathBuf,
    pub paths: Vec<String>,
    pub exclude: Vec<Pattern>,
    pub limits: Limits,
    /// Every rule with its level, in 0.4.0's order.
    pub rules: Vec<(&'static str, Level)>,
    pub boundaries: Vec<Boundary>,
    pub relaxed_warnings: Vec<String>,
    pub gate: Gate,
}

impl Config {
    /// The defaults, rooted at `root`.
    pub fn new(root: PathBuf) -> Config {
        Config {
            root,
            paths: vec![".".into()],
            exclude: settings::DEFAULT_EXCLUDE
                .iter()
                .map(|p| Pattern::new(p).expect("a valid default"))
                .collect(),
            limits: Limits {
                cognitive: 8,
                cyclomatic: 16,
                comment_lines: 2,
                inputs: 8,
            },
            rules: settings::DEFAULT_RULES.to_vec(),
            boundaries: Vec::new(),
            relaxed_warnings: Vec::new(),
            gate: Gate {
                format: true,
                lint: true,
                steps: Vec::new(),
                import: true,
                analyzer: true,
                tests: Vec::new(),
                test_commands: Vec::new(),
                smoke: vec!["--quit-after".into(), "3".into()],
                jobs: Jobs::Count(1),
            },
        }
    }

    pub fn baseline_path(&self) -> PathBuf {
        self.root.join(BASELINE_NAME)
    }

    /// The rule's level; a rule without a setting, such as `boundary`, is an error.
    pub fn level(&self, rule: &str) -> Level {
        self.rules
            .iter()
            .find(|(name, _)| *name == rule)
            .map_or(Level::Error, |(_, level)| *level)
    }

    pub fn is_scanned(&self, relative: &str) -> bool {
        if !relative.ends_with(".gd") || hidden(relative) || matches(relative, &self.exclude) {
            return false;
        }
        self.paths.iter().any(|p| {
            p == "."
                || relative == p
                || relative.starts_with(&format!("{}/", p.trim_end_matches('/')))
        })
    }

    pub fn scanned_files(&self) -> Vec<String> {
        let mut found: Vec<String> = self
            .paths
            .iter()
            .flat_map(|entry| walk_scripts(&self.root, &self.root.join(entry)))
            .filter(|relative| self.is_scanned(relative))
            .collect();
        found.sort();
        found.dedup();
        found
    }
}

/// Whether a directory on the root-relative path is hidden; a trailing `/` marks the path itself as one.
pub fn hidden(relative: &str) -> bool {
    let parts: Vec<&str> = relative.split('/').collect();
    parts[..parts.len() - 1]
        .iter()
        .any(|part| part.starts_with('.'))
}

/// fnmatch globs (`*` crosses `/`), and a bare directory matches everything below it.
pub fn matches(relative: &str, patterns: &[Pattern]) -> bool {
    patterns.iter().any(|p| {
        p.matches(relative) || relative.starts_with(&format!("{}/", p.text.trim_end_matches('/')))
    })
}

/// Repo-relative `.gd` paths under `start`, skipping hidden directories. Files keep the directory's
/// listing order and subdirectories are walked sorted, as `os.walk` with sorted subdirectories does;
/// symlinked directories are listed but not entered.
pub fn walk_scripts(root: &Path, start: &Path) -> Vec<String> {
    if start.is_file() {
        return vec![relative_posix(root, start)];
    }
    let mut found = Vec::new();
    walk_directory(root, start, &mut found);
    found
}

fn walk_directory(root: &Path, directory: &Path, found: &mut Vec<String>) {
    let Ok(listing) = fs::read_dir(directory) else {
        return;
    };
    let mut subdirectories = Vec::new();
    for entry in listing.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            subdirectories.extend(enters(&entry, &name).then_some(name));
        } else if name.ends_with(".gd") {
            found.push(relative_posix(root, &path));
        }
    }
    subdirectories.sort();
    for name in subdirectories {
        walk_directory(root, &directory.join(name), found);
    }
}

/// Whether the walk enters a directory: not hidden, not a symlink.
fn enters(entry: &fs::DirEntry, name: &str) -> bool {
    !name.starts_with('.') && !entry.file_type().is_ok_and(|t| t.is_symlink())
}

/// `path.relative_to(root).as_posix()`, lexically; `.` components drop out as pathlib drops them.
pub fn relative_posix(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let parts: Vec<_> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect();
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

/// The nearest directory holding the config, else one holding project.godot.
pub fn find_root(start: &Path) -> Result<(PathBuf, bool)> {
    let start = fs::canonicalize(start).unwrap_or_else(|_| start.to_path_buf());
    for directory in start.ancestors() {
        if directory.join(CONFIG_NAME).is_file() {
            return Ok((directory.to_path_buf(), true));
        }
    }
    for directory in start.ancestors() {
        if directory.join("project.godot").is_file() {
            return Ok((directory.to_path_buf(), false));
        }
    }
    Err(Failure::Config(format!(
        "no {CONFIG_NAME} or project.godot in {} or its parents",
        start.display()
    )))
}

pub fn load(start: &Path) -> Result<Config> {
    let (root, has_config) = find_root(start)?;
    let mut config = Config::new(root);
    if !has_config {
        return Ok(config);
    }
    let path = config.root.join(CONFIG_NAME);
    let bytes = fs::read(&path).map_err(|e| Failure::crash(path.display(), e))?;
    let text = String::from_utf8(bytes).map_err(|e| Failure::crash(path.display(), e))?;
    let data = crate::yaml::load(&builtins::universal_newlines(&text))
        .map_err(|error| Failure::Config(format!("{CONFIG_NAME}: {error}")))?;
    settings::apply(&mut config, data)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_limit_reads_and_writes_its_own_field() {
        let mut limits = Config::new(PathBuf::new()).limits;
        for (value, limit) in (100..).zip(Limit::ALL) {
            limits.set(limit, value);
        }
        assert_eq!(
            limits.entries(),
            [
                ("cognitive", 100),
                ("cyclomatic", 101),
                ("comment-lines", 102),
                ("inputs", 103)
            ]
        );
    }

    #[test]
    fn hidden_looks_at_directories_only() {
        assert!(hidden(".godot/a.gd"));
        assert!(hidden("a/.b/c.gd"));
        assert!(!hidden(".a.gd"));
        assert!(hidden("scripts/.hidden/"));
    }

    #[test]
    fn a_bare_directory_matches_below_it() {
        let patterns = [
            Pattern::new("scripts/gen").unwrap(),
            Pattern::new("addons/**").unwrap(),
        ];
        assert!(matches("scripts/gen/b.gd", &patterns));
        assert!(matches("addons/x/c.gd", &patterns));
        assert!(!matches("scripts/general.gd", &patterns));
    }
}
