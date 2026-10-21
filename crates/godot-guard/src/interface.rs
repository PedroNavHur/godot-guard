//! Count a GDScript script's configuration surface, as pedro-best-practices `godot_interface.py` does.
//!
//! A script's configuration surface is what a parent decides when it uses the script: its top-level
//! `@export` variables (group, subgroup, and category annotations are not variables), its signals, and
//! the parameters of `_init` and of a `setup` or `configure` method. Resource scripts are data
//! definitions and are excluded: a script whose `extends` chain reaches `Resource` through the
//! repository's `class_name` declarations. Inner classes are not counted.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use gdrules::interface::{NOT_VARIABLES, is_export_annotation};
use regex::Regex;

const SETUP_METHODS: [&str; 3] = ["_init", "setup", "configure"];

fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("a valid built-in pattern")
}

static TOKENS: LazyLock<Regex> =
    LazyLock::new(|| compile(r#""[^"]*(?:"|\z)|'[^']*(?:'|\z)|[()\[\]{},]"#));
static LINE: LazyLock<Regex> = LazyLock::new(|| compile(r"(?m)^\S.*$"));
static EXTENDS: LazyLock<Regex> = LazyLock::new(|| compile(r#"^extends\s+([\w."/]+)"#));
static CLASS_NAME: LazyLock<Regex> =
    LazyLock::new(|| compile(r"^class_name\s+(\w+)(?:\s+extends\s+([\w.]+))?"));
static SIGNAL: LazyLock<Regex> = LazyLock::new(|| compile(r"^signal\s+(\w+)"));
static VARIABLE: LazyLock<Regex> = LazyLock::new(|| compile(r"(^|\s)var\s+\w+"));
static VARIABLE_NAME: LazyLock<Regex> = LazyLock::new(|| compile(r"var\s+(\w+)"));
static ANNOTATION: LazyLock<Regex> = LazyLock::new(|| compile(r"@(\w+)"));
static FUNCTION: LazyLock<Regex> = LazyLock::new(|| compile(r"^(?:static\s+)?func\s+(\w+)\s*\("));
static PARAMETER_END: LazyLock<Regex> = LazyLock::new(|| compile(r"[:=\s]"));

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Surface {
    pub path: String,
    pub extends: String,
    pub class_name: String,
    pub signals: Vec<String>,
    pub exports: Vec<String>,
    /// Parameter names by setup method, in the order the methods first appear.
    pub parameters: Vec<(String, Vec<String>)>,
}

impl Surface {
    pub fn total(&self) -> usize {
        self.signals.len()
            + self.exports.len()
            + self.parameters.iter().map(|(_, p)| p.len()).sum::<usize>()
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        if !self.exports.is_empty() {
            parts.push(format!("@export {}", self.exports.len()));
        }
        if !self.signals.is_empty() {
            parts.push(format!("signals {}", self.signals.len()));
        }
        parts.extend(
            self.parameters
                .iter()
                .filter(|(_, names)| !names.is_empty())
                .map(|(m, names)| format!("{m}() {}", names.len())),
        );
        parts.join(", ")
    }
}

/// Split on commas outside brackets and strings.
fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i64, 0);
    for token in TOKENS.find_iter(text) {
        depth += match token.as_str() {
            "(" | "[" | "{" => 1,
            ")" | "]" | "}" => -1,
            _ => 0,
        };
        if token.as_str() == "," && depth == 0 {
            parts.push(&text[start..token.start()]);
            start = token.end();
        }
    }
    parts.push(&text[start..]);
    parts
        .into_iter()
        .map(crate::py::strip)
        .filter(|p| !p.is_empty())
        .collect()
}

/// Parameter names of the signature whose `(` is at or after `start`.
fn parameters_after(source: &str, start: usize) -> Vec<String> {
    let open_at = start
        + source[start..]
            .find('(')
            .expect("the signature's line holds its `(`");
    let mut depth = 0i64;
    for (offset, c) in source[open_at..].char_indices() {
        depth += match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        if depth == 0 {
            let inside = &source[open_at + 1..open_at + offset];
            return split_top_level(inside)
                .into_iter()
                .map(|p| PARAMETER_END.splitn(p, 2).next().unwrap_or("").to_owned())
                .collect();
        }
    }
    Vec::new()
}

fn read_header(surface: &mut Surface, line: &str) {
    if let Some(found) = EXTENDS.captures(line) {
        surface.extends = found[1].trim_matches('"').to_owned();
    }
    if let Some(found) = CLASS_NAME.captures(line) {
        surface.class_name = found[1].to_owned();
        if let Some(parent) = found.get(2) {
            surface.extends = parent.as_str().to_owned();
        }
    }
    if let Some(found) = SIGNAL.captures(line) {
        surface.signals.push(found[1].to_owned());
    }
}

fn is_variable(line: &str) -> bool {
    VARIABLE.is_match(line) && !(line.starts_with("func") || line.starts_with("static func"))
}

/// Record an exported variable on `line`; return the annotations still waiting for a var.
fn read_exports(surface: &mut Surface, line: &str, pending: Vec<String>) -> Vec<String> {
    let annotations: Vec<String> = if line.starts_with('@') {
        let before_var = line.split_once("var ").map_or(line, |(head, _)| head);
        ANNOTATION
            .captures_iter(before_var)
            .map(|c| c[1].to_owned())
            .collect()
    } else {
        Vec::new()
    };
    if is_variable(line) {
        if annotations
            .iter()
            .chain(&pending)
            .any(|a| is_export_annotation(a))
        {
            let name = VARIABLE_NAME
                .captures(line)
                .expect("a variable line names its variable");
            surface.exports.push(name[1].to_owned());
        }
        return Vec::new();
    }
    if !annotations.is_empty() && !line.contains("var ") {
        let mut waiting = pending;
        waiting.extend(
            annotations
                .into_iter()
                .filter(|a| !NOT_VARIABLES.contains(&a.as_str())),
        );
        return waiting;
    }
    Vec::new()
}

pub fn parse(path: &str, source: &str) -> Surface {
    let mut surface = Surface {
        path: path.to_owned(),
        ..Surface::default()
    };
    let mut pending = Vec::new();
    for found in LINE.find_iter(source) {
        let line = found.as_str();
        if line.starts_with('#') {
            continue;
        }
        read_header(&mut surface, line);
        pending = read_exports(&mut surface, line, pending);
        if let Some(function) = FUNCTION.captures(line)
            && SETUP_METHODS.contains(&&function[1])
        {
            let names = parameters_after(source, found.start());
            match surface
                .parameters
                .iter_mut()
                .find(|(method, _)| method == &function[1])
            {
                Some((_, existing)) => *existing = names,
                None => surface.parameters.push((function[1].to_owned(), names)),
            }
        }
    }
    surface
}

/// Whether the script's `extends` chain reaches `Resource` through `class_name`s or res:// paths.
pub fn is_resource(
    surface: &Surface,
    by_class: &HashMap<String, Surface>,
    by_path: &HashMap<String, Surface>,
) -> bool {
    let mut seen = HashSet::new();
    let mut current = surface.extends.clone();
    while !current.is_empty() && !seen.contains(&current) {
        if current == "Resource" {
            return true;
        }
        let parent = by_class
            .get(&current)
            .or_else(|| by_path.get(&current.replace("res://", "")));
        seen.insert(current);
        current = parent.map(|p| p.extends.clone()).unwrap_or_default();
    }
    false
}
