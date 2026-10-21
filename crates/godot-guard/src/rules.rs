//! Run every rule on the scripts and collect measured items and direct problems.
//!
//! Files are analyzed in parallel, then meet the Resource map in path order: 0.4.0 adds each
//! analyzed script to it.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gdrules::complexity::{self, Function};
use gdrules::patterns;
use rayon::prelude::*;

use crate::comments;
use crate::config::{Boundary, Config, Level, matches, walk_scripts};
use crate::error::{Failure, Result};
use crate::interface::{self, Surface};
use crate::parse::Parsed;
use crate::py;
use crate::scripts::Scripts;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub path: String,
    pub line: usize,
    pub message: String,
    pub rule: &'static str,
    pub level: Level,
}

impl Problem {
    pub fn new(
        path: impl Into<String>,
        line: usize,
        message: String,
        rule: &'static str,
        level: Level,
    ) -> Problem {
        Problem {
            path: path.into(),
            line,
            message,
            rule,
            level,
        }
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}:{}: {} ({})",
            self.path, self.line, self.message, self.rule
        )
    }
}

/// One baselined measurement: a value checked against a limit and the baseline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub rule: &'static str,
    pub key: String,
    pub path: String,
    pub line: usize,
    pub value: usize,
    pub label: String,
    pub locations: Vec<(usize, String)>,
}

#[derive(Debug)]
pub struct FileResult {
    pub path: String,
    pub items: Vec<Item>,
    pub problems: Vec<Problem>,
    pub parsed: bool,
}

/// Interface surfaces of scripts by root-relative path and by class_name, for the Resource exclusion.
///
/// Every script under the root is read when `paths` is None; unreadable scripts are skipped.
pub struct Surfaces {
    pub by_path: HashMap<String, Surface>,
    pub by_class: HashMap<String, Surface>,
}

impl Surfaces {
    pub fn new(scripts: &Scripts, root: &Path, paths: Option<Vec<String>>) -> Surfaces {
        let read = read_surfaces(
            scripts,
            root,
            paths.unwrap_or_else(|| walk_scripts(root, root)),
        );
        let by_class = classes(&read);
        Surfaces {
            by_path: read.into_iter().map(|s| (s.path.clone(), s)).collect(),
            by_class,
        }
    }

    /// Read `paths` under `root` over the current scripts; their paths and class_names win.
    pub fn overlay(&mut self, scripts: &Scripts, root: &Path, paths: Vec<String>) {
        let read = read_surfaces(scripts, root, paths);
        self.by_class.extend(classes(&read));
        self.by_path
            .extend(read.into_iter().map(|s| (s.path.clone(), s)));
    }

    /// The script's surface, or None for a Resource script.
    pub fn surface(&mut self, surface: Surface) -> Option<Surface> {
        self.by_path
            .entry(surface.path.clone())
            .or_insert_with(|| surface.clone());
        (!interface::is_resource(&surface, &self.by_class, &self.by_path)).then_some(surface)
    }
}

/// Surfaces of the readable scripts, in `paths` order.
fn read_surfaces(scripts: &Scripts, root: &Path, paths: Vec<String>) -> Vec<Surface> {
    let absolute: Vec<PathBuf> = paths.iter().map(|relative| root.join(relative)).collect();
    let texts = scripts.texts(&absolute);
    paths
        .into_par_iter()
        .zip(texts)
        .filter_map(|(relative, text)| Some(interface::parse(&relative, &text.ok()?)))
        .collect()
}

/// By class_name; a later script wins a repeated name.
fn classes(surfaces: &[Surface]) -> HashMap<String, Surface> {
    surfaces
        .iter()
        .filter(|s| !s.class_name.is_empty())
        .map(|s| (s.class_name.clone(), s.clone()))
        .collect()
}

fn function_metrics(path: &str, name: &str, function: &Function) -> Vec<Item> {
    let metrics = [
        ("cognitive", function.cognitive),
        ("cyclomatic", Some(function.cyclomatic)),
    ];
    metrics
        .into_iter()
        .filter_map(|(metric, value)| {
            let value = value?;
            Some(Item {
                rule: if metric == "cognitive" {
                    "cognitive-complexity"
                } else {
                    "cyclomatic-complexity"
                },
                key: format!("{path}::{name}"),
                path: path.to_owned(),
                line: function.line,
                value,
                label: format!("{name} has {metric} complexity {value}"),
                locations: Vec::new(),
            })
        })
        .collect()
}

/// Function names, with `#N` on every occurrence of a name repeated in the file.
pub fn unique_names(functions: &[Function]) -> Vec<String> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for function in functions {
        *counts.entry(&function.name).or_default() += 1;
    }
    let mut seen: HashMap<&str, usize> = HashMap::new();
    functions
        .iter()
        .map(|function| {
            let number = seen.entry(&function.name).or_default();
            *number += 1;
            match counts[function.name.as_str()] {
                1 => function.name.clone(),
                _ => format!("{}#{number}", function.name),
            }
        })
        .collect()
}

/// Cognitive and cyclomatic items, one key per function.
pub fn function_items(path: &str, functions: &[Function]) -> Vec<Item> {
    let names = unique_names(functions);
    names
        .iter()
        .zip(functions)
        .flat_map(|(name, function)| function_metrics(path, name, function))
        .collect()
}

fn tree_rules(result: &mut FileResult, parsed: &Parsed) {
    let tree = match parsed {
        Parsed::Tree(tree) => tree,
        Parsed::Error { line, message } => {
            result.parsed = false;
            let message = format!("cannot parse: {message}");
            result.problems.push(Problem::new(
                &result.path,
                *line,
                message,
                "parse-error",
                Level::Error,
            ));
            return;
        }
    };
    result
        .items
        .extend(function_items(&result.path, &complexity::score(tree)));
    for line in complexity::nested_ternaries(tree) {
        let message = "nested conditional expression; use if/else, match, or a helper".to_owned();
        result.problems.push(Problem::new(
            &result.path,
            line,
            message,
            "nested-ternary",
            Level::Warn,
        ));
    }
    pattern_rules(result, &patterns::scan(tree));
}

/// One counted item per file for each COUNTED rule, located at its hits; a warning per ADVISORY hit.
fn pattern_rules(result: &mut FileResult, hits: &patterns::Hits) {
    for (rule, noun) in patterns::COUNTED {
        let found = hits.get(rule).unwrap_or_default();
        result.items.push(Item {
            rule,
            key: result.path.clone(),
            path: result.path.clone(),
            line: found.first().map_or(1, |(line, _)| *line),
            value: found.len(),
            label: format!("{} {noun}", found.len()),
            locations: found.to_vec(),
        });
    }
    for rule in patterns::ADVISORY {
        for (line, message) in hits.get(rule).unwrap_or_default() {
            result.problems.push(Problem::new(
                &result.path,
                *line,
                message.clone(),
                rule,
                Level::Warn,
            ));
        }
    }
}

fn comment_item(config: &Config, path: &str, source: &str) -> Item {
    let cap = config.limits.comment_lines;
    let found = comments::long_comments(source, cap);
    Item {
        rule: "long-comment",
        key: path.to_owned(),
        path: path.to_owned(),
        line: found.first().map_or(1, |c| c.line),
        value: found.len(),
        label: format!("{} long comments", found.len()),
        locations: found
            .iter()
            .map(|c| {
                (
                    c.line,
                    format!("comment has {} prose lines (max {cap})", c.prose()),
                )
            })
            .collect(),
    }
}

fn boundary_problems(config: &Config, path: &str, source: &str) -> Result<Vec<Problem>> {
    let stripped = comments::strip_comments(source);
    let lines = py::splitlines(&stripped);
    let mut problems = Vec::new();
    for entry in config
        .boundaries
        .iter()
        .filter(|entry| matches(path, &entry.paths))
    {
        problems.extend(boundary_hits(entry, path, &lines)?);
    }
    Ok(problems)
}

fn boundary_hits(entry: &Boundary, path: &str, lines: &[&str]) -> Result<Vec<Problem>> {
    let suffix = if entry.message.is_empty() {
        String::new()
    } else {
        format!("; {}", entry.message)
    };
    let mut problems = Vec::new();
    for (index, code) in lines.iter().enumerate() {
        for pattern in &entry.forbid {
            let found = pattern
                .find(code)
                .map_err(|e| Failure::crash(format!("boundary {}", entry.name), e))?;
            if let Some(found) = found {
                let message = format!(
                    "{}: '{}' is forbidden here{suffix}",
                    entry.name,
                    found.as_str()
                );
                problems.push(Problem::new(
                    path,
                    index + 1,
                    message,
                    "boundary",
                    Level::Error,
                ));
            }
        }
    }
    Ok(problems)
}

/// Everything but the interface item, which waits for the Resource map.
fn analyze_one(
    config: &Config,
    path: &str,
    source: &str,
    parsed: &Parsed,
) -> Result<(FileResult, Surface)> {
    let mut result = FileResult {
        path: path.to_owned(),
        items: Vec::new(),
        problems: Vec::new(),
        parsed: true,
    };
    tree_rules(&mut result, parsed);
    result.items.push(comment_item(config, path, source));
    result
        .problems
        .extend(boundary_problems(config, path, source)?);
    Ok((result, interface::parse(path, source)))
}

/// Analyze the root-relative `paths`, in order.
pub fn analyze(
    config: &Config,
    paths: &[String],
    surfaces: &mut Surfaces,
    scripts: &Scripts,
) -> Result<Vec<FileResult>> {
    let absolute: Vec<PathBuf> = paths.iter().map(|path| config.root.join(path)).collect();
    let texts: Vec<Arc<str>> = scripts
        .texts(&absolute)
        .into_iter()
        .zip(&absolute)
        .map(|(text, path)| text.map_err(|e| Failure::crash(path.display(), e)))
        .collect::<Result<_>>()?;
    let trees = scripts.trees(&absolute, &texts);
    let analyzed: Vec<(FileResult, Surface)> = (paths, &texts, &trees)
        .into_par_iter()
        .map(|(path, text, parsed)| analyze_one(config, path, text, parsed))
        .collect::<Result<_>>()?;
    Ok(analyzed
        .into_iter()
        .map(|(mut result, surface)| {
            if let Some(surface) = surfaces.surface(surface) {
                let total = surface.total();
                let detail = if total > 0 {
                    format!(" ({})", surface.describe())
                } else {
                    String::new()
                };
                result.items.push(Item {
                    rule: "interface-cap",
                    key: result.path.clone(),
                    path: result.path.clone(),
                    line: 1,
                    value: total,
                    label: format!("{total} inputs{detail}"),
                    locations: Vec::new(),
                });
            }
            result
        })
        .collect())
}
