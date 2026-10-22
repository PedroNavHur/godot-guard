//! The config: `DEFAULT_CONFIG` from `linter/__init__.py`, and `.gdlintrc` search and loading from
//! `linter/__main__.py`.
//!
//! The Python reads config values where a check uses them, so a value of the wrong type crashes the first
//! lint. Here every value an enabled check will read is checked once, at load, and a bad one is an error
//! naming the key; values of disabled checks stay unread, as in the Python.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::pyre::FullMatch;
use crate::yaml::{self, Value};

const PASCAL_CASE: &str = "([A-Z][a-z0-9]*)+";
const SNAKE_CASE: &str = "[a-z][a-z0-9]*(_[a-z0-9]+)*";
const PRIVATE_SNAKE_CASE: &str = "_?[a-z][a-z0-9]*(_[a-z0-9]+)*";
const UPPER_SNAKE_CASE: &str = "[A-Z][A-Z0-9]*(_[A-Z0-9]+)*";
const PRIVATE_UPPER_SNAKE_CASE: &str = "_?[A-Z][A-Z0-9]*(_[A-Z0-9]+)*";

pub const CONFIG_FILE_NAME: &str = "gdlintrc";

/// What `gdlint -d` writes: `yaml.dump(DEFAULT_CONFIG.copy())`, keys sorted, captured from gdtoolkit 4.5.0.
pub const DEFAULT_CONFIG_DUMP: &str = include_str!("default_gdlintrc.yaml");

/// The name checks, in the order `name_checks.lint` runs them.
pub const NAME_CHECKS: [&str; 14] = [
    "function-name",
    "sub-class-name",
    "class-name",
    "signal-name",
    "enum-name",
    "enum-element-name",
    "loop-variable-name",
    "function-argument-name",
    "function-variable-name",
    "function-preload-variable-name",
    "constant-name",
    "load-constant-name",
    "class-variable-name",
    "class-load-variable-name",
];

/// `DEFAULT_CONFIG`, in its order, which `--verbose` prints.
pub fn default_entries() -> Vec<(&'static str, Value)> {
    let text = |s: &str| Value::Str(s.to_owned());
    let list = |items: &[&str]| Value::List(items.iter().map(|s| text(s)).collect());
    vec![
        ("disable", Value::List(Vec::new())),
        (
            "function-name",
            text(&format!(
                "(_on_{PASCAL_CASE}(_[a-z0-9]+)*|{PRIVATE_SNAKE_CASE})"
            )),
        ),
        ("class-name", text(PASCAL_CASE)),
        ("sub-class-name", text(&format!("_?{PASCAL_CASE}"))),
        ("signal-name", text(SNAKE_CASE)),
        ("class-variable-name", text(PRIVATE_SNAKE_CASE)),
        (
            "class-load-variable-name",
            text(&format!("({PASCAL_CASE}|{PRIVATE_SNAKE_CASE})")),
        ),
        ("function-variable-name", text(SNAKE_CASE)),
        ("function-preload-variable-name", text(PASCAL_CASE)),
        ("function-argument-name", text(PRIVATE_SNAKE_CASE)),
        ("loop-variable-name", text(PRIVATE_SNAKE_CASE)),
        ("enum-name", text(PASCAL_CASE)),
        ("enum-element-name", text(UPPER_SNAKE_CASE)),
        ("constant-name", text(PRIVATE_UPPER_SNAKE_CASE)),
        (
            "load-constant-name",
            text(&format!("({PASCAL_CASE}|{PRIVATE_UPPER_SNAKE_CASE})")),
        ),
        ("duplicated-load", Value::Null),
        ("expression-not-assigned", Value::Null),
        ("unnecessary-pass", Value::Null),
        ("unused-argument", Value::Null),
        ("comparison-with-itself", Value::Null),
        (
            "class-definitions-order",
            list(&[
                "tools",
                "classnames",
                "extends",
                "docstrings",
                "signals",
                "enums",
                "consts",
                "staticvars",
                "exports",
                "pubvars",
                "prvvars",
                "onreadypubvars",
                "onreadyprvvars",
                "others",
            ]),
        ),
        ("max-returns", Value::Int(6)),
        ("max-public-methods", Value::Int(20)),
        ("function-arguments-number", Value::Int(10)),
        ("max-file-lines", Value::Int(1000)),
        ("trailing-whitespace", Value::Null),
        ("max-line-length", Value::Int(100)),
        ("tab-characters", Value::Int(1)),
        ("mixed-tabs-and-spaces", Value::Null),
        ("excluded_directories", Value::Set(vec![text(".git")])),
        ("no-elif-return", Value::Null),
        ("no-else-return", Value::Null),
    ]
}

#[derive(Debug, PartialEq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

/// `config["disable"]`, read with Python's `in`.
#[derive(Clone, Debug)]
enum Disable {
    /// A list, set or dict: the check names among its items or keys.
    Names(Vec<String>),
    /// A string: `name in text` is a substring test.
    Text(String),
}

/// A loaded config, ready for `lint_code`: every enabled check's settings already read and checked.
#[derive(Clone, Debug)]
pub struct Config {
    disable: Disable,
    name_patterns: HashMap<&'static str, FullMatch>,
    ints: HashMap<&'static str, i64>,
    class_definitions_order: Vec<String>,
    /// `set(config["excluded_directories"])`, for walking directories.
    pub excluded_directories: Vec<String>,
}

impl Config {
    /// gdlint's defaults.
    pub fn defaults() -> Config {
        let entries = default_entries()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        Config::from_entries(entries).expect("the default config is valid")
    }

    /// A config from a `.gdlintrc`'s top-level mapping, missing keys taken from the defaults, as
    /// `_update_config_with_missing_entries_inplace` does.
    pub fn from_entries(mut entries: HashMap<String, Value>) -> Result<Config, ConfigError> {
        for (key, value) in default_entries() {
            entries.entry(key.to_owned()).or_insert(value);
        }
        let disable = disable(&entries)?;
        let excluded_directories = excluded_directories(&entries["excluded_directories"])?;
        let name_patterns = name_patterns(&entries, &disable)?;
        let ints = int_settings(&entries, &disable)?;
        let class_definitions_order = class_definitions_order(&entries, &disable)?;
        Ok(Config {
            disable,
            name_patterns,
            ints,
            class_definitions_order,
            excluded_directories,
        })
    }

    /// Whether `check` runs: `check not in config["disable"]`.
    pub fn is_enabled(&self, check: &str) -> bool {
        self.disable.enables(check)
    }

    /// The pattern of an enabled name check.
    pub fn name_pattern(&self, check: &str) -> &FullMatch {
        &self.name_patterns[check]
    }

    /// An integer setting of an enabled check.
    pub fn int(&self, key: &str) -> i64 {
        self.ints[key]
    }

    pub fn class_definitions_order(&self) -> &[String] {
        &self.class_definitions_order
    }
}

impl Disable {
    /// `check not in config["disable"]`.
    fn enables(&self, check: &str) -> bool {
        match self {
            Disable::Names(names) => !names.iter().any(|name| name == check),
            Disable::Text(text) => !text.contains(check),
        }
    }
}

/// The int settings each check reads, by check.
const INT_SETTINGS: [(&str, &[&str]); 5] = [
    ("max-returns", &["max-returns"]),
    ("max-public-methods", &["max-public-methods"]),
    ("function-arguments-number", &["function-arguments-number"]),
    ("max-file-lines", &["max-file-lines"]),
    ("max-line-length", &["max-line-length", "tab-characters"]),
];

fn disable(entries: &HashMap<String, Value>) -> Result<Disable, ConfigError> {
    match &entries["disable"] {
        Value::List(items) | Value::Set(items) => Ok(Disable::Names(strings(items))),
        Value::Dict(pairs) => Ok(Disable::Names(strings(pairs.iter().map(|(k, _)| k)))),
        Value::Str(text) => Ok(Disable::Text(text.clone())),
        other => Err(type_error("disable", "a list of check names", other)),
    }
}

fn name_patterns(
    entries: &HashMap<String, Value>,
    disable: &Disable,
) -> Result<HashMap<&'static str, FullMatch>, ConfigError> {
    let mut name_patterns = HashMap::new();
    for check in NAME_CHECKS
        .into_iter()
        .filter(|check| disable.enables(check))
    {
        let Value::Str(pattern) = &entries[check] else {
            return Err(type_error(check, "a regular expression", &entries[check]));
        };
        let compiled = FullMatch::new(pattern).map_err(|e| ConfigError(format!("{check}: {e}")))?;
        name_patterns.insert(check, compiled);
    }
    Ok(name_patterns)
}

fn int_settings(
    entries: &HashMap<String, Value>,
    disable: &Disable,
) -> Result<HashMap<&'static str, i64>, ConfigError> {
    let mut ints = HashMap::new();
    let keys = INT_SETTINGS
        .into_iter()
        .filter(|(check, _)| disable.enables(check))
        .flat_map(|(_, keys)| keys);
    for &key in keys {
        let Value::Int(number) = entries[key] else {
            return Err(type_error(key, "an integer", &entries[key]));
        };
        ints.insert(key, number);
    }
    Ok(ints)
}

fn class_definitions_order(
    entries: &HashMap<String, Value>,
    disable: &Disable,
) -> Result<Vec<String>, ConfigError> {
    if !disable.enables("class-definitions-order") {
        return Ok(Vec::new());
    }
    match &entries["class-definitions-order"] {
        Value::List(items)
            if !items.is_empty() && items.iter().all(|i| matches!(i, Value::Str(_))) =>
        {
            Ok(strings(items))
        }
        other => Err(type_error(
            "class-definitions-order",
            "a non-empty list of sections",
            other,
        )),
    }
}

fn strings<'a>(items: impl IntoIterator<Item = &'a Value>) -> Vec<String> {
    items
        .into_iter()
        .filter_map(|item| match item {
            Value::Str(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn excluded_directories(value: &Value) -> Result<Vec<String>, ConfigError> {
    match value {
        Value::List(items) | Value::Set(items) => Ok(strings(items)),
        Value::Dict(pairs) => Ok(strings(pairs.iter().map(|(k, _)| k))),
        Value::Str(text) => Ok(text.chars().map(String::from).collect()),
        other => Err(type_error(
            "excluded_directories",
            "a list or set of names",
            other,
        )),
    }
}

fn type_error(key: &str, expected: &str, found: &Value) -> ConfigError {
    ConfigError(format!(
        "config entry {key:?} must be {expected}, not {}",
        crate::repr::value_repr(found)
    ))
}

/// `_find_config_file`: `gdlintrc`, then `.gdlintrc`, in `cwd` and each parent, stopping before the root,
/// which the Python never searches.
pub fn find_config_file(cwd: &Path) -> Option<PathBuf> {
    let mut search_dir = Some(cwd);
    while let Some(dir) = search_dir.filter(|dir| dir.parent().is_some()) {
        for name in [CONFIG_FILE_NAME.to_owned(), format!(".{CONFIG_FILE_NAME}")] {
            let path = dir.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
        search_dir = dir.parent();
    }
    None
}

/// A `.gdlintrc`'s text, read as `yaml.load` does, as top-level entries in document order.
pub fn parse_entries(text: &str) -> Result<Vec<(Value, Value)>, ConfigError> {
    match yaml::load(text).map_err(|e| ConfigError(e.to_string()))? {
        Some(Value::Dict(pairs)) => Ok(pairs),
        None => Err(ConfigError("the config file is empty".into())),
        Some(other) => Err(ConfigError(format!(
            "the config file must hold a mapping, not {}",
            crate::repr::value_repr(&other)
        ))),
    }
}

/// The entries `Config::from_entries` reads: those with string keys, the only ones a lookup finds.
pub fn string_keyed(entries: &[(Value, Value)]) -> HashMap<String, Value> {
    entries
        .iter()
        .filter_map(|(key, value)| match key {
            Value::Str(key) => Some((key.clone(), value.clone())),
            _ => None,
        })
        .collect()
}
