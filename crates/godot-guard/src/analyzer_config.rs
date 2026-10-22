//! `analyzer-config`: project.godot must set the analyzer warnings in `godot_warnings::REQUIRED` to error.
//!
//! The analyzer step fails only on warnings at error: Godot prints nothing for a lower level when it
//! loads a script headless, so a warning left at warn is never checked.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::config::{Config, Level, matches};
use crate::error::{Failure, Result};
use crate::godot_warnings::{ERROR, REQUIRED};
use crate::rules::Problem;
use pycompat::{builtins, string};

pub const PROJECT: &str = "project.godot";
pub const RULE: &str = "analyzer-config";

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a valid built-in pattern")
}

static SECTION: LazyLock<Regex> = LazyLock::new(|| compile(r"^\[([^\]]+)\]\s*$"));
static WARNING: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^gdscript/warnings/(\w+)\s*=\s*(.*?)\s*$"));
static DIRECTORY_RULES: LazyLock<Regex> =
    LazyLock::new(|| compile(r"gdscript/warnings/directory_rules\s*=\s*\{([^}]*)\}"));
static DIRECTORY_ENTRY: LazyLock<Regex> =
    LazyLock::new(|| compile(r#""res://([^"]*)"\s*:\s*(\d+)"#));

struct Setting {
    line: usize,
    value: String,
}

type Finding = (usize, String);

/// The `[debug]` section's numbered lines, its header first.
fn debug_lines(text: &str) -> Vec<(usize, &str)> {
    let mut inside = false;
    let mut found = Vec::new();
    for (index, line) in string::splitlines(text).into_iter().enumerate() {
        if let Some(header) = SECTION.captures(line) {
            inside = &header[1] == "debug";
        }
        if inside {
            found.push((index + 1, line));
        }
    }
    found
}

/// `gdscript/warnings/*` settings by name; a repeated name keeps its last value.
fn read_settings(lines: &[(usize, &str)]) -> HashMap<String, Setting> {
    lines
        .iter()
        .filter_map(|(number, line)| {
            let found = WARNING.captures(line)?;
            Some((
                found[1].to_owned(),
                Setting {
                    line: *number,
                    value: found[2].to_owned(),
                },
            ))
        })
        .collect()
}

fn lowered(name: &str, setting: &Setting) -> Finding {
    let message = format!(
        "gdscript/warnings/{name} is {}; set it to {ERROR}: the analyzer step ignores lower levels",
        setting.value
    );
    (setting.line, message)
}

fn absent(names: &[&str], line: usize) -> Finding {
    let message = format!(
        "{} analyzer warnings default below error: add gdscript/warnings/NAME={ERROR} under [debug] for {}",
        names.len(),
        names.join(", ")
    );
    (line, message)
}

fn disabled(settings: &HashMap<String, Setting>) -> Vec<Finding> {
    match settings.get("enable") {
        Some(enable) if enable.value != "true" => {
            vec![(
                enable.line,
                "gdscript/warnings/enable turns every analyzer warning off; remove the setting"
                    .into(),
            )]
        }
        _ => Vec::new(),
    }
}

/// (line, message) for each `directory_rules` entry that turns warnings off under a path the scan covers.
fn directory_rules_off(lines: &[(usize, &str)], config: &Config) -> Vec<Finding> {
    let body = lines
        .iter()
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n");
    let mut found = Vec::new();
    for setting in DIRECTORY_RULES.captures_iter(&body) {
        let start = setting.get(0).expect("the whole match").start();
        let line = lines[body[..start].matches('\n').count()].0;
        for entry in DIRECTORY_ENTRY.captures_iter(&setting[1]) {
            let path = &entry[1];
            if &entry[2] == "0" && !matches(&format!("{path}/"), &config.exclude) {
                let message = format!(
                    "gdscript/warnings/directory_rules turns analyzer warnings off under res://{path}; remove that entry"
                );
                found.push((line, message));
            }
        }
    }
    found
}

/// (line, message) for warnings off, each required one set below error, and those left at a lower default.
fn findings(
    settings: &HashMap<String, Setting>,
    relaxed: &[String],
    header: usize,
) -> Vec<Finding> {
    let mut found = disabled(settings);
    let checked: Vec<(&str, u8)> = REQUIRED
        .iter()
        .copied()
        .filter(|(name, _)| !relaxed.iter().any(|r| r == name))
        .collect();
    let error = ERROR.to_string();
    found.extend(checked.iter().filter_map(|(name, _)| {
        let setting = settings.get(*name)?;
        (setting.value != error).then(|| lowered(name, setting))
    }));
    let missing: Vec<&str> = checked
        .iter()
        .filter(|(name, default)| !settings.contains_key(*name) && *default != ERROR)
        .map(|(name, _)| *name)
        .collect();
    if !missing.is_empty() {
        found.push(absent(&missing, header));
    }
    found
}

/// The project's analyzer settings; none without a project.godot or with the rule off.
pub fn problems(config: &Config) -> Result<Vec<Problem>> {
    let path = config.root.join(PROJECT);
    let level = config.level(RULE);
    if level == Level::Off || !path.is_file() {
        return Ok(Vec::new());
    }
    let text = builtins::read_text_replace(&path).map_err(|e| Failure::crash(path.display(), e))?;
    let lines = debug_lines(&text);
    let header = lines.first().map_or(1, |(number, _)| *number);
    let mut found = findings(&read_settings(&lines), &config.relaxed_warnings, header);
    found.extend(directory_rules_off(&lines, config));
    Ok(found
        .into_iter()
        .map(|(line, message)| Problem::new(PROJECT, line, message, RULE, level))
        .collect())
}
