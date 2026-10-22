//! The ratchet baseline: measured items over their limit, which may only shrink.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use gdrules::patterns::COUNTED;

use crate::config::{BASELINE_NAME, Config, Level, Limit};
use crate::error::{Failure, Result};
use crate::json::{self, Json};
use crate::rules::{FileResult, Item, Problem};
use crate::settings::baselined;
use pycompat::{builtins, string};

const TREE_RULES: [&str; 2] = ["cognitive-complexity", "cyclomatic-complexity"];

/// The limit an item's value is checked against. Counted rules (long-comment, the pattern rules) count
/// hits, so any is over.
pub fn limit(config: &Config, rule: &str) -> i64 {
    limit_of(rule).map_or(0, |limit| config.limits.get(limit))
}

fn limit_of(rule: &str) -> Option<Limit> {
    match rule {
        "cognitive-complexity" => Some(Limit::Cognitive),
        "cyclomatic-complexity" => Some(Limit::Cyclomatic),
        "interface-cap" => Some(Limit::Inputs),
        _ => None,
    }
}

/// File rules key by path; function names never hold `::`, so the path ends at the last one.
pub fn item_path<'a>(rule: &str, key: &'a str) -> &'a str {
    match key.rsplit_once("::") {
        Some((path, _)) if TREE_RULES.contains(&rule) => path,
        _ => key,
    }
}

/// Measured on the parse tree: a file that fails to parse has no item, so its entries are not missing.
fn is_parsed_rule(rule: &str) -> bool {
    TREE_RULES.contains(&rule) || COUNTED.iter().any(|(counted, _)| *counted == rule)
}

/// Recorded values by key, per baselined rule in the file's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Data(Vec<(&'static str, BTreeMap<String, i64>)>);

impl Data {
    pub fn empty() -> Data {
        Data(
            baselined()
                .into_iter()
                .map(|rule| (rule, BTreeMap::new()))
                .collect(),
        )
    }

    pub fn rule(&self, rule: &str) -> &BTreeMap<String, i64> {
        &self
            .0
            .iter()
            .find(|(name, _)| *name == rule)
            .expect("a baselined rule")
            .1
    }

    pub fn rule_mut(&mut self, rule: &str) -> &mut BTreeMap<String, i64> {
        &mut self
            .0
            .iter_mut()
            .find(|(name, _)| *name == rule)
            .expect("a baselined rule")
            .1
    }

    /// (rule, key, recorded) for every entry, keys sorted within each rule.
    pub fn entries(&self) -> impl Iterator<Item = (&'static str, &str, i64)> {
        self.0.iter().flat_map(|(rule, keys)| {
            keys.iter()
                .map(move |(key, recorded)| (*rule, key.as_str(), *recorded))
        })
    }

    /// The file's text: sorted keys, indent 2, and a trailing newline.
    pub fn dump(&self) -> String {
        let mut fields = vec![("version".to_owned(), Json::Int(1))];
        fields.extend(self.0.iter().map(|(rule, keys)| {
            let entries = keys
                .iter()
                .map(|(key, value)| (key.clone(), Json::Int(*value)))
                .collect();
            (rule.to_string(), Json::Object(entries))
        }));
        json::dumps_sorted(&Json::Object(fields)) + "\n"
    }
}

/// ceiling: 0.4.0 compares any JSON number it finds; this reads whole numbers only and stops on others.
fn recorded(rule: &str, key: &str, value: &serde_json::Value) -> Result<i64> {
    value.as_i64().ok_or_else(|| {
        Failure::Crash(format!(
            "{BASELINE_NAME}: {rule} {key:?} is not a whole number: {value}"
        ))
    })
}

pub fn load(path: &Path) -> Result<Data> {
    let mut data = Data::empty();
    if !path.is_file() {
        return Ok(data);
    }
    let text = fs::read_to_string(path).map_err(|e| Failure::crash(path.display(), e))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| Failure::crash(BASELINE_NAME, e))?;
    let object = parsed
        .as_object()
        .ok_or_else(|| Failure::crash(BASELINE_NAME, "not a JSON object"))?;
    for rule in baselined() {
        let Some(keys) = object.get(rule) else {
            continue;
        };
        let keys = keys
            .as_object()
            .ok_or_else(|| Failure::crash(BASELINE_NAME, format!("{rule} is not an object")))?;
        for (key, value) in keys {
            data.rule_mut(rule)
                .insert(key.clone(), recorded(rule, key, value)?);
        }
    }
    Ok(data)
}

pub fn write(path: &Path, data: &Data) -> Result<()> {
    fs::write(path, data.dump()).map_err(|e| Failure::crash(path.display(), e))
}

fn entry_line(text: &str, key: &str) -> usize {
    let needle = json::quote(key) + ":";
    string::splitlines(text)
        .iter()
        .position(|line| line.contains(&needle))
        .map_or(1, |index| index + 1)
}

/// Current values by (rule, key), and the paths that failed to parse.
fn measured(results: &[FileResult]) -> (HashMap<(&'static str, &str), usize>, HashSet<&str>) {
    let current = results
        .iter()
        .flat_map(|r| &r.items)
        .map(|i| ((i.rule, i.key.as_str()), i.value))
        .collect();
    let unparsed = results
        .iter()
        .filter(|r| !r.parsed)
        .map(|r| r.path.as_str())
        .collect();
    (current, unparsed)
}

/// A missing entry counts when its file is in the subset and parsed.
fn in_scope(
    rule: &str,
    key: &str,
    subset: Option<&HashSet<String>>,
    unparsed: &HashSet<&str>,
) -> bool {
    let path = item_path(rule, key);
    if subset.is_some_and(|subset| !subset.contains(path)) {
        return false;
    }
    !(is_parsed_rule(rule) && unparsed.contains(path))
}

/// Compares measured items with their limits and the baseline.
pub struct Judge<'a> {
    config: &'a Config,
    data: Data,
    text: String,
}

impl<'a> Judge<'a> {
    pub fn new(config: &'a Config) -> Result<Judge<'a>> {
        let path = config.baseline_path();
        let text = if path.is_file() {
            fs::read_to_string(&path).map_err(|e| Failure::crash(path.display(), e))?
        } else {
            String::new()
        };
        Ok(Judge {
            config,
            data: load(&path)?,
            text,
        })
    }

    pub fn item(&self, item: &Item) -> Vec<Problem> {
        let level = self.config.level(item.rule);
        let allowed = limit(self.config, item.rule);
        let value = i64::try_from(item.value).expect("a count fits in i64");
        let recorded = self.data.rule(item.rule).get(&item.key).copied();
        let problem = |message: String| {
            vec![Problem::new(
                &item.path, item.line, message, item.rule, level,
            )]
        };
        match recorded {
            _ if value <= allowed => match recorded {
                None => Vec::new(),
                Some(_) => problem(format!(
                    "{} is within the limit {allowed}: remove its baseline entry with `godot-guard baseline`",
                    item.label
                )),
            },
            Some(recorded) if value < recorded => problem(format!(
                "{}, below its baseline {recorded}: lower it with `godot-guard baseline`",
                item.label
            )),
            Some(recorded) if value == recorded => Vec::new(),
            _ => self.over(item, allowed, recorded, level),
        }
    }

    fn over(&self, item: &Item, allowed: i64, recorded: Option<i64>, level: Level) -> Vec<Problem> {
        let suffix = match recorded {
            None => format!(" (max {allowed})"),
            Some(recorded) => format!(" (baseline {recorded}, max {allowed})"),
        };
        if item.locations.is_empty() {
            return vec![Problem::new(
                &item.path,
                item.line,
                format!("{}{suffix}", item.label),
                item.rule,
                level,
            )];
        }
        let note = match recorded {
            None => String::new(),
            Some(recorded) => format!(
                "; the file has {}, its baseline allows {recorded}",
                item.label
            ),
        };
        item.locations
            .iter()
            .map(|(line, text)| {
                Problem::new(&item.path, *line, format!("{text}{note}"), item.rule, level)
            })
            .collect()
    }

    /// Baseline entries whose item no longer exists, within `subset` (None: everywhere).
    pub fn missing(
        &self,
        results: &[FileResult],
        subset: Option<&HashSet<String>>,
    ) -> Vec<Problem> {
        let (present, unparsed) = measured(results);
        self.data
            .entries()
            .filter(|(rule, _, _)| self.config.level(rule) != Level::Off)
            .filter(|(rule, key, _)| {
                !present.contains_key(&(*rule, *key)) && in_scope(rule, key, subset, &unparsed)
            })
            .map(|(rule, key, _)| self.gone(rule, key))
            .collect()
    }

    fn gone(&self, rule: &'static str, key: &str) -> Problem {
        let message = format!(
            "baseline entry {} no longer exists: remove it with `godot-guard baseline`",
            builtins::repr(key)
        );
        Problem::new(
            BASELINE_NAME,
            entry_line(&self.text, key),
            message,
            rule,
            self.config.level(rule),
        )
    }
}

/// Every item over its limit, as baseline data.
pub fn violations(config: &Config, results: &[FileResult]) -> Data {
    let mut data = Data::empty();
    for item in results.iter().flat_map(|r| &r.items) {
        let value = i64::try_from(item.value).expect("a count fits in i64");
        if config.level(item.rule) != Level::Off && value > limit(config, item.rule) {
            data.rule_mut(item.rule).insert(item.key.clone(), value);
        }
    }
    data
}

/// Drop stale entries and lower loose ones; never add or raise.
pub fn tighten(config: &Config, data: &Data, results: &[FileResult]) -> (Data, Vec<String>) {
    let (current, unparsed) = measured(results);
    let mut tightened = Data::empty();
    let mut changes = Vec::new();
    for (rule, key, recorded) in data.entries() {
        let mut value = current
            .get(&(rule, key))
            .map(|value| i64::try_from(*value).expect("a count fits in i64"));
        if value.is_none() && !in_scope(rule, key, None, &unparsed) {
            value = Some(recorded);
        }
        let (kept, change) = settle(rule, key, recorded, value, limit(config, rule));
        if let Some(kept) = kept {
            tightened.rule_mut(rule).insert(key.to_owned(), kept);
        }
        changes.extend(change);
    }
    (tightened, changes)
}

fn settle(
    rule: &str,
    key: &str,
    recorded: i64,
    value: Option<i64>,
    allowed: i64,
) -> (Option<i64>, Option<String>) {
    match value {
        None => (
            None,
            Some(format!("removed {rule} {key}: no longer exists")),
        ),
        Some(value) if value <= allowed => (
            None,
            Some(format!(
                "removed {rule} {key}: {value} is within the limit {allowed}"
            )),
        ),
        Some(value) if value < recorded => (
            Some(value),
            Some(format!("lowered {rule} {key}: {recorded} -> {value}")),
        ),
        Some(_) => (Some(recorded), None),
    }
}
