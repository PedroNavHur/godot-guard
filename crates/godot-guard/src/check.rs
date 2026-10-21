//! `godot-guard check` and `godot-guard baseline`.

use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::analyzer_config;
use crate::baseline::{self, Judge};
use crate::config::{BASELINE_NAME, Config, Level, hidden, matches, relative_posix, walk_scripts};
use crate::error::Result;
use crate::json::{self, Json};
use crate::rules::{self, FileResult, Problem, Surfaces};
use crate::scripts::Scripts;
use crate::settings::baselined;

const JSON_VERSION: i64 = 1;

/// Repo-relative scanned scripts among `files` (paths relative to the working directory).
pub fn select(config: &Config, files: &[String]) -> Vec<String> {
    let chosen: BTreeSet<String> = files
        .iter()
        .flat_map(|name| expand(config, root_relative(config, name)))
        .collect();
    chosen.into_iter().collect()
}

/// `os.path.abspath`: joined to the working directory and normalized without resolving symlinks.
fn abspath(name: &str) -> PathBuf {
    let joined = env::current_dir().unwrap_or_default().join(name);
    let mut normal = PathBuf::from("/");
    for component in joined.components() {
        match component {
            Component::ParentDir => {
                normal.pop();
            }
            Component::Normal(part) => normal.push(part),
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    normal
}

/// `Path.resolve()`: symlinks resolved as far as the path exists, the rest appended.
fn resolve(path: &Path) -> PathBuf {
    if let Ok(resolved) = fs::canonicalize(path) {
        return resolved;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => resolve(parent).join(name),
        _ => path.to_path_buf(),
    }
}

/// `name` relative to the root, without resolving symlinks below the root; None outside it.
fn root_relative(config: &Config, name: &str) -> Option<String> {
    let path = abspath(name);
    let mut ancestors: Vec<&Path> = path.ancestors().collect();
    ancestors.reverse();
    ancestors
        .into_iter()
        .find(|ancestor| resolve(ancestor) == config.root)
        .map(|ancestor| relative_posix(ancestor, &path))
}

/// Scanned scripts at `relative`: the file itself, or every one below a directory.
fn expand(config: &Config, relative: Option<String>) -> Vec<String> {
    let Some(relative) = relative else {
        return Vec::new();
    };
    let path = config.root.join(&relative);
    if !path.exists() {
        return Vec::new();
    }
    if let Some(reason) = skip_reason(config, &relative, path.is_dir()) {
        eprintln!("godot-guard: skipping {relative}: {reason}");
        return Vec::new();
    }
    walk_scripts(&config.root, &path)
        .into_iter()
        .filter(|relative| config.is_scanned(relative))
        .collect()
}

/// Why an argument selects nothing: it or a directory above it is hidden or excluded.
fn skip_reason(config: &Config, relative: &str, is_dir: bool) -> Option<&'static str> {
    if relative == "." {
        return None;
    }
    let probe = if is_dir {
        format!("{relative}/")
    } else {
        relative.to_owned()
    };
    if hidden(&probe) {
        return Some("hidden directories are not scanned");
    }
    matches(&probe, &config.exclude).then_some("it matches exclude")
}

pub fn measure(
    config: &Config,
    paths: &[String],
    surfaces: Option<Surfaces>,
    scripts: &Scripts,
) -> Result<Vec<FileResult>> {
    let mut surfaces = surfaces.unwrap_or_else(|| Surfaces::new(scripts, &config.root, None));
    rules::analyze(config, paths, &mut surfaces, scripts)
}

fn problems_of(
    config: &Config,
    results: &[FileResult],
    subset: Option<&HashSet<String>>,
) -> Result<Vec<Problem>> {
    let judge = Judge::new(config)?;
    let mut problems = Vec::new();
    for result in results {
        problems.extend(
            result
                .problems
                .iter()
                .filter(|p| config.level(p.rule) != Level::Off)
                .cloned(),
        );
        for item in result
            .items
            .iter()
            .filter(|item| config.level(item.rule) != Level::Off)
        {
            problems.extend(judge.item(item));
        }
    }
    problems.extend(judge.missing(results, subset));
    Ok(problems)
}

/// What `check` asks for: which files, which Resource map, and which output.
#[derive(Default)]
pub struct Request {
    /// Paths relative to the working directory; None checks every scanned script and project.godot.
    pub files: Option<Vec<String>>,
    /// The Resource map, read from the root's scripts when None.
    pub surfaces: Option<Surfaces>,
    pub as_json: bool,
}

/// Run the rules; the status (1 on any error) and the report's text.
pub fn check(config: &Config, request: Request, scripts: &Scripts) -> Result<(i32, String)> {
    let paths = match &request.files {
        None => config.scanned_files(),
        Some(files) => select(config, files),
    };
    let subset: Option<HashSet<String>> = request
        .files
        .as_ref()
        .map(|_| paths.iter().cloned().collect());
    let results = measure(config, &paths, request.surfaces, scripts)?;
    let mut problems = problems_of(config, &results, subset.as_ref())?;
    if request.files.is_none() {
        problems.extend(analyzer_config::problems(config)?);
    }
    problems.sort_by(|a, b| (&a.path, a.line, a.rule).cmp(&(&b.path, b.line, b.rule)));
    let text = if request.as_json {
        json::dumps(&report(config, &paths, &problems)) + "\n"
    } else {
        print_text(&paths, &problems)
    };
    Ok((
        i32::from(problems.iter().any(|p| p.level == Level::Error)),
        text,
    ))
}

fn print_text(paths: &[String], problems: &[Problem]) -> String {
    let mut out = String::new();
    for problem in problems {
        let prefix = if problem.level == Level::Warn {
            "warning: "
        } else {
            ""
        };
        out += &format!(
            "{}:{}: {prefix}{} ({})\n",
            problem.path, problem.line, problem.message, problem.rule
        );
    }
    let errors = problems.iter().filter(|p| p.level == Level::Error).count();
    let warnings = problems.len() - errors;
    out += &format!(
        "godot-guard check: {}, {}, {}\n",
        plural(errors, "error"),
        plural(warnings, "warning"),
        plural(paths.len(), "file")
    );
    out
}

/// `check --json`: the effective settings, the checked files, and every problem `check` prints.
fn report(config: &Config, paths: &[String], problems: &[Problem]) -> Json {
    let text = |value: &str| Json::Str(value.to_owned());
    let limits = config
        .limits
        .entries()
        .iter()
        .map(|(name, value)| (name.to_string(), Json::Int(*value)))
        .collect();
    let levels = config
        .rules
        .iter()
        .map(|(rule, level)| (rule.to_string(), text(level.as_str())))
        .collect();
    let problem = |p: &Problem| {
        Json::Object(vec![
            ("path".into(), text(&p.path)),
            (
                "line".into(),
                Json::Int(i64::try_from(p.line).expect("a line fits in i64")),
            ),
            ("rule".into(), text(p.rule)),
            ("level".into(), text(p.level.as_str())),
            ("message".into(), text(&p.message)),
        ])
    };
    Json::Object(vec![
        ("version".into(), Json::Int(JSON_VERSION)),
        ("root".into(), text(&config.root.to_string_lossy())),
        ("limits".into(), Json::Object(limits)),
        ("levels".into(), Json::Object(levels)),
        (
            "files".into(),
            Json::List(paths.iter().map(|p| text(p)).collect()),
        ),
        (
            "problems".into(),
            Json::List(problems.iter().map(problem).collect()),
        ),
    ])
}

pub fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("{count} {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// `godot-guard baseline`: the status and what it prints.
pub fn update_baseline(
    config: &Config,
    init: bool,
    force: bool,
    scripts: &Scripts,
) -> Result<(i32, String)> {
    let path = config.baseline_path();
    if init && path.exists() && !force {
        return Ok((
            1,
            format!("{BASELINE_NAME} already exists; pass --force to overwrite it\n"),
        ));
    }
    if !init && !path.exists() {
        let message = format!(
            "no {BASELINE_NAME}; nothing to tighten (create one with `godot-guard baseline --init`)\n"
        );
        return Ok((0, message));
    }
    let results = measure(config, &config.scanned_files(), None, scripts)?;
    if init {
        let data = baseline::violations(config, &results);
        baseline::write(&path, &data)?;
        let counts: Vec<String> = baselined()
            .into_iter()
            .map(|rule| format!("{rule} {}", data.rule(rule).len()))
            .collect();
        return Ok((0, format!("wrote {BASELINE_NAME}: {}\n", counts.join(", "))));
    }
    let (data, changes) = baseline::tighten(config, &baseline::load(&path)?, &results);
    let mut out: String = changes.iter().map(|change| format!("{change}\n")).collect();
    if !changes.is_empty() {
        baseline::write(&path, &data)?;
    }
    out += &format!("{BASELINE_NAME}: {}\n", plural(changes.len(), "change"));
    Ok((0, out))
}
