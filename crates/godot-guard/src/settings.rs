//! Validate a loaded `.godot-guard.yml` against the defaults, in 0.4.0's order and with its messages,
//! and apply it to a `Config`.

use crate::config::{AUTO, Boundary, CONFIG_NAME, Config, Jobs, Level, Limits, TestScript};
use crate::error::{Failure, Result};
use crate::fnmatch::Pattern;
use crate::godot_warnings;
use crate::patterns::{ADVISORY as PATTERN_ADVISORY, COUNTED};
use crate::py;
use crate::yaml::Value;

pub const DEFAULT_EXCLUDE: [&str; 2] = ["addons/**", ".godot/**"];
pub const DEFAULT_RULES: [(&str, Level); 11] = [
    ("cognitive-complexity", Level::Error),
    ("cyclomatic-complexity", Level::Error),
    ("long-comment", Level::Error),
    ("interface-cap", Level::Error),
    ("nested-ternary", Level::Warn),
    ("analyzer-config", Level::Error),
    ("lookup-in-hot-path", Level::Error),
    ("load-in-hot-path", Level::Error),
    ("private-export", Level::Error),
    ("self-assign", Level::Error),
    ("per-frame-allocation", Level::Warn),
];
const TOP_KEYS: [&str; 7] = [
    "paths",
    "exclude",
    "limits",
    "rules",
    "relaxed-warnings",
    "boundaries",
    "gate",
];
const GATE_KEYS: [&str; 8] = [
    "format", "lint", "steps", "import", "analyzer", "tests", "smoke", "jobs",
];
const BOUNDARY_KEYS: [&str; 4] = ["name", "paths", "forbid", "message"];
const TEST_KEYS: [&str; 2] = ["script", "shards"];
const LEVELS: [Level; 3] = [Level::Error, Level::Warn, Level::Off];

/// The baselined rules, in the baseline file's order.
pub fn baselined() -> Vec<&'static str> {
    let fixed = [
        "cognitive-complexity",
        "cyclomatic-complexity",
        "long-comment",
        "interface-cap",
    ];
    fixed
        .into_iter()
        .chain(COUNTED.iter().map(|(rule, _)| *rule))
        .collect()
}

/// Rules that may only warn.
pub fn advisory() -> Vec<&'static str> {
    std::iter::once("nested-ternary")
        .chain(PATTERN_ADVISORY.iter().copied())
        .collect()
}

fn fail(message: impl Into<String>) -> Failure {
    Failure::Config(format!("{CONFIG_NAME}: {}", message.into()))
}

fn expect(condition: bool, message: impl FnOnce() -> String) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(fail(message()))
    }
}

type Mapping = [(Value, Value)];

/// `data.get(key)`, where None is also an explicit null: the caller takes the default.
fn given<'a>(data: &'a Mapping, key: &str) -> Option<&'a Value> {
    data.iter()
        .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
        .map(|(_, v)| v)
        .filter(|v| **v != Value::Null)
}

fn unknown(values: &Mapping, known: &[&str], prefix: &str) -> Result<()> {
    for (key, _) in values {
        let is_known = matches!(key, Value::Str(s) if known.contains(&s.as_str()));
        expect(is_known, || {
            format!("unknown key '{prefix}{}'", key.py_str())
        })?;
    }
    Ok(())
}

fn string_list(value: &Value, name: &str) -> Result<Vec<String>> {
    let strings = match value {
        Value::Seq(items) => items
            .iter()
            .map(|v| {
                if let Value::Str(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect(),
        _ => None,
    };
    strings.ok_or_else(|| fail(format!("{name} must be a list of strings")))
}

fn section<'a>(data: &'a Mapping, name: &str, known: &[&str]) -> Result<Vec<(&'a str, &'a Value)>> {
    let values = match given(data, name) {
        None => return Ok(Vec::new()),
        Some(Value::Map(values)) => values,
        Some(_) => return Err(fail(format!("{name} must be a mapping"))),
    };
    unknown(values, known, &format!("{name}."))?;
    Ok(values
        .iter()
        .filter(|(_, v)| *v != Value::Null)
        .map(|(k, v)| {
            (
                if let Value::Str(s) = k {
                    s.as_str()
                } else {
                    unreachable!("checked by unknown")
                },
                v,
            )
        })
        .collect())
}

/// A whole number of at least 1; a bool is not a number here.
fn whole_count(value: &Value) -> Option<usize> {
    match value {
        Value::Int(number) if *number >= 1 => usize::try_from(*number).ok(),
        _ => None,
    }
}

fn normalize(path: &str) -> String {
    let mut path = py::strip(path).trim_end_matches('/');
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    if path.is_empty() {
        ".".into()
    } else {
        path.into()
    }
}

pub fn apply(config: &mut Config, data: Option<Value>) -> Result<()> {
    let data = match data {
        None => Vec::new(),
        Some(Value::Map(entries)) => entries,
        Some(_) => return Err(fail("the file must be a mapping")),
    };
    unknown(&data, &TOP_KEYS, "")?;
    if let Some(paths) = given(&data, "paths") {
        config.paths = string_list(paths, "paths")?
            .iter()
            .map(|p| normalize(p))
            .collect();
    }
    let exclude = match given(&data, "exclude") {
        Some(exclude) => Some(
            string_list(exclude, "exclude")?
                .iter()
                .map(|p| normalize(p))
                .collect::<Vec<_>>(),
        ),
        None => None,
    };
    let limits = section(&data, "limits", &Limits::NAMES)?;
    let rules = section(&data, "rules", &DEFAULT_RULES.map(|(rule, _)| rule))?;
    let gate = section(&data, "gate", &GATE_KEYS)?;
    let boundaries = match given(&data, "boundaries") {
        None => Vec::new(),
        Some(Value::Seq(entries)) => entries.iter().map(boundary).collect::<Result<Vec<_>>>()?,
        Some(_) => return Err(fail("boundaries must be a list")),
    };
    config.relaxed_warnings = match given(&data, "relaxed-warnings") {
        Some(value) => relaxed(value)?,
        None => Vec::new(),
    };
    validate_limits(config, &limits)?;
    validate_rules(config, &rules)?;
    validate_gate(config, &gate)?;
    if let Some(exclude) = exclude {
        config.exclude = exclude
            .iter()
            .map(|p| glob(p, "exclude"))
            .collect::<Result<_>>()?;
    }
    config.boundaries = boundaries;
    Ok(())
}

/// A compiled glob. 0.4.0 compiles globs as it matches and crashes on a bad one; this reports it here.
fn glob(text: &str, setting: &str) -> Result<Pattern> {
    Pattern::new(text).map_err(|error| {
        fail(format!(
            "{setting}: bad pattern {}: {error}",
            py::repr(text)
        ))
    })
}

fn boundary(entry: &Value) -> Result<Boundary> {
    let Value::Map(entry) = entry else {
        return Err(fail("each boundary must be a mapping"));
    };
    for (key, _) in entry {
        let is_known = matches!(key, Value::Str(s) if BOUNDARY_KEYS.contains(&s.as_str()));
        expect(is_known, || {
            format!("unknown key 'boundaries.{}'", key.py_str())
        })?;
    }
    let name = given(entry, "name").map_or_else(|| "boundary".into(), Value::py_str);
    let empty = Value::Seq(Vec::new());
    let forbid_list = given(entry, "forbid").unwrap_or(&empty);
    let mut forbid = Vec::new();
    for pattern in string_list(forbid_list, &format!("boundary {name} forbid"))? {
        let compiled = fancy_regex::Regex::new(&pattern).map_err(|error| {
            Failure::Config(format!(
                "{CONFIG_NAME}: boundary {name}: bad pattern {}: {error}",
                py::repr(&pattern)
            ))
        })?;
        forbid.push(compiled);
    }
    let paths = string_list(
        given(entry, "paths").unwrap_or(&empty),
        &format!("boundary {name} paths"),
    )?;
    let paths = paths
        .iter()
        .map(|p| glob(p, &format!("boundary {name} paths")))
        .collect::<Result<_>>()?;
    let message = given(entry, "message").map_or_else(String::new, Value::py_str);
    Ok(Boundary {
        name,
        paths,
        forbid,
        message,
    })
}

/// `relaxed-warnings`: names from godot_warnings::REQUIRED that project.godot may keep below error.
fn relaxed(value: &Value) -> Result<Vec<String>> {
    let names = string_list(value, "relaxed-warnings")?;
    for name in &names {
        let message = || {
            format!(
                "relaxed-warnings: {} is not a warning analyzer-config requires",
                py::repr(name)
            )
        };
        expect(godot_warnings::is_required(name), message)?;
    }
    Ok(names)
}

fn validate_limits(config: &mut Config, given: &[(&str, &Value)]) -> Result<()> {
    for name in Limits::NAMES {
        let Some((_, value)) = given.iter().find(|(key, _)| *key == name) else {
            continue;
        };
        let Value::Int(number) = value else {
            return Err(fail(format!("limits.{name} must be a whole number")));
        };
        expect(*number >= 0, || {
            format!("limits.{name} must be a whole number")
        })?;
        let limits = &mut config.limits;
        *match name {
            "cognitive" => &mut limits.cognitive,
            "cyclomatic" => &mut limits.cyclomatic,
            "comment-lines" => &mut limits.comment_lines,
            _ => &mut limits.inputs,
        } = *number;
    }
    Ok(())
}

fn validate_rules(config: &mut Config, given: &[(&str, &Value)]) -> Result<()> {
    for (rule, level) in config.rules.iter_mut() {
        let Some((_, value)) = given.iter().find(|(key, _)| key == rule) else {
            continue;
        };
        // YAML 1.1 reads a bare `off` as false.
        let parsed = match value {
            Value::Bool(false) => Some(Level::Off),
            Value::Str(text) => LEVELS.into_iter().find(|level| level.as_str() == text),
            _ => None,
        };
        *level =
            parsed.ok_or_else(|| fail(format!("rules.{rule} must be one of error, warn, off")))?;
    }
    for rule in advisory() {
        expect(config.level(rule) != Level::Error, || {
            format!("rules.{rule} is advisory: use warn or off")
        })?;
    }
    Ok(())
}

fn validate_gate(config: &mut Config, given: &[(&str, &Value)]) -> Result<()> {
    let value = |key: &str| given.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
    let gate = &mut config.gate;
    for (key, flag) in [
        ("format", &mut gate.format),
        ("lint", &mut gate.lint),
        ("import", &mut gate.import),
    ]
    .into_iter()
    .chain([("analyzer", &mut gate.analyzer)])
    {
        match value(key) {
            None => {}
            Some(Value::Bool(setting)) => *flag = *setting,
            Some(_) => return Err(fail(format!("gate.{key} must be true or false"))),
        }
    }
    if let Some(jobs) = value("jobs") {
        gate.jobs = match jobs {
            Value::Str(text) if text == AUTO => Jobs::Auto,
            _ => Jobs::Count(whole_count(jobs).ok_or_else(|| {
                fail(format!(
                    "gate.jobs must be a whole number of at least 1, or {AUTO}"
                ))
            })?),
        };
    }
    if let Some(steps) = value("steps") {
        gate.steps = string_list(steps, "gate.steps")?;
    }
    if let Some(tests) = value("tests") {
        let Value::Seq(entries) = tests else {
            return Err(fail("gate.tests must be a list"));
        };
        gate.tests = entries.iter().map(script_entry).collect::<Result<_>>()?;
    }
    if let Some(smoke) = value("smoke") {
        gate.smoke = match smoke {
            Value::Bool(false) => Vec::new(),
            Value::Seq(arguments) => arguments.iter().map(Value::py_str).collect(),
            _ => return Err(fail("gate.smoke must be a list of arguments or false")),
        };
    }
    Ok(())
}

/// A `gate.tests` entry: a script path, or a mapping with `script` and an optional `shards` count.
fn script_entry(entry: &Value) -> Result<TestScript> {
    let entry = match entry {
        Value::Str(script) => {
            return Ok(TestScript {
                script: script.clone(),
                shards: None,
            });
        }
        Value::Map(entry) => entry,
        _ => return Err(fail("each gate.tests entry must be a string or a mapping")),
    };
    unknown(entry, &TEST_KEYS, "gate.tests.")?;
    let Some(Value::Str(script)) = given(entry, "script") else {
        return Err(fail("gate.tests script must be a string"));
    };
    let shards = match given(entry, "shards") {
        None => None,
        Some(count) => Some(
            whole_count(count)
                .ok_or_else(|| fail("gate.tests shards must be a whole number of at least 1"))?,
        ),
    };
    Ok(TestScript {
        script: script.clone(),
        shards,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_dot_slashes_and_trailing_slashes() {
        assert_eq!(normalize(" ./scripts/ "), "scripts");
        assert_eq!(normalize("././a"), "a");
        assert_eq!(normalize("./"), ".");
    }

    #[test]
    fn the_baselined_rules_keep_their_order() {
        assert_eq!(
            baselined()[..4],
            [
                "cognitive-complexity",
                "cyclomatic-complexity",
                "long-comment",
                "interface-cap"
            ]
        );
        assert_eq!(baselined().len(), 8);
    }
}
