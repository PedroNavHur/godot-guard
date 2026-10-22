//! `yaml.load(text, Loader=yaml.Loader)` as PyYAML 6 does it, for the values gdlint's and gdformat's configs
//! (`.gdlintrc`, `gdformatrc`) hold, and `repr` of those values.
//!
//! saphyr-parser reads the syntax; this module builds the values with PyYAML's YAML 1.1 rules, which differ
//! from YAML 1.2's: `yes`/`on` are booleans, `017` is octal, `1_000` and `1:30` are integers, and `!!set` makes
//! a set. Constructs a config never needs (merge keys, timestamps, `!!python/...` tags) fail loudly.

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

use regex::Regex;
use saphyr_parser::{Event, Parser, ScalarStyle, Tag};

use crate::builtins::{float_repr, repr};

/// A value as PyYAML constructs it, in the Python types that matter to a config.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    /// A dict, in insertion order; a repeated key keeps its first position and takes the last value.
    Dict(Vec<(Value, Value)>),
    /// A set, from `!!set`, in the order the document lists it.
    Set(Vec<Value>),
}

#[derive(Debug, PartialEq)]
pub struct YamlError(pub String);

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for YamlError {}

fn error<T>(message: impl Into<String>) -> Result<T, YamlError> {
    Err(YamlError(message.into()))
}

/// The single document in `text`, or None for an empty stream, as `yaml.load` returns it.
pub fn load(text: &str) -> Result<Option<Value>, YamlError> {
    let mut builder = Builder::default();
    let mut documents = Vec::new();
    for event in Parser::new_from_str(text) {
        let (event, _) = event.map_err(|e| YamlError(e.to_string()))?;
        match event {
            Event::DocumentEnd => documents.push(builder.finish()?),
            other => builder.event(other)?,
        }
    }
    if documents.len() > 1 {
        return error("expected a single document in the stream");
    }
    Ok(documents.pop())
}

enum Frame {
    List(usize, Vec<Value>),
    Dict(usize, bool, Vec<(Value, Value)>, Option<Value>),
}

#[derive(Default)]
struct Builder {
    stack: Vec<Frame>,
    anchors: HashMap<usize, Value>,
    root: Option<Value>,
}

impl Builder {
    fn event(&mut self, event: Event<'_>) -> Result<(), YamlError> {
        match event {
            Event::Scalar(text, style, anchor, tag) => {
                let value = scalar(&text, style, tag.as_deref())?;
                self.complete(anchor, value)
            }
            Event::Alias(anchor) => match self.anchors.get(&anchor) {
                Some(value) => self.add(value.clone()),
                None => error(format!("found undefined alias {anchor}")),
            },
            Event::SequenceStart(anchor, tag) => {
                collection_tag(tag.as_deref(), "seq")?;
                self.stack.push(Frame::List(anchor, Vec::new()));
                Ok(())
            }
            Event::MappingStart(anchor, tag) => {
                let is_set = collection_tag(tag.as_deref(), "map")?;
                self.stack
                    .push(Frame::Dict(anchor, is_set, Vec::new(), None));
                Ok(())
            }
            Event::SequenceEnd | Event::MappingEnd => {
                let (anchor, value) = match self.stack.pop() {
                    Some(Frame::List(anchor, items)) => (anchor, Value::List(items)),
                    Some(Frame::Dict(anchor, true, pairs, _)) => (
                        anchor,
                        Value::Set(pairs.into_iter().map(|(k, _)| k).collect()),
                    ),
                    Some(Frame::Dict(anchor, false, pairs, _)) => (anchor, Value::Dict(pairs)),
                    None => return error("collection end without a start"),
                };
                self.complete(anchor, value)
            }
            Event::StreamStart | Event::StreamEnd | Event::DocumentStart(_) | Event::Nothing => {
                Ok(())
            }
            Event::DocumentEnd => unreachable!("load handles document ends"),
        }
    }

    fn complete(&mut self, anchor: usize, value: Value) -> Result<(), YamlError> {
        if anchor != 0 {
            self.anchors.insert(anchor, value.clone());
        }
        self.add(value)
    }

    fn add(&mut self, value: Value) -> Result<(), YamlError> {
        match self.stack.last_mut() {
            None => self.root = Some(value),
            Some(Frame::List(_, items)) => items.push(value),
            Some(Frame::Dict(_, _, pairs, pending)) => match pending.take() {
                None => *pending = Some(mapping_key(value)?),
                Some(key) => insert_pair(pairs, key, value),
            },
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<Value, YamlError> {
        self.anchors.clear();
        Ok(self.root.take().unwrap_or(Value::Null))
    }
}

/// `value` as a mapping key; merge keys and unhashable values are refused.
fn mapping_key(value: Value) -> Result<Value, YamlError> {
    if value == Value::Str("<<".into()) {
        return error("merge keys (<<) are not supported");
    }
    if matches!(value, Value::List(_) | Value::Dict(_) | Value::Set(_)) {
        return error("found unhashable key");
    }
    Ok(value)
}

/// PyYAML's repeated-key rule: the key keeps its first position and takes the last value.
fn insert_pair(pairs: &mut Vec<(Value, Value)>, key: Value, value: Value) {
    match pairs.iter_mut().find(|(k, _)| *k == key) {
        Some(pair) => pair.1 = value,
        None => pairs.push((key, value)),
    }
}

/// Whether a mapping is a `!!set`; any other explicit collection tag is refused.
fn collection_tag(tag: Option<&Tag>, kind: &str) -> Result<bool, YamlError> {
    let Some(tag) = tag else { return Ok(false) };
    if tag.is_yaml_core_schema() && (tag.suffix == kind || (kind == "map" && tag.suffix == "set")) {
        return Ok(tag.suffix == "set");
    }
    if tag.handle.is_empty() && tag.suffix == "!" {
        return Ok(false);
    }
    error(format!("unsupported tag {tag}"))
}

fn scalar(text: &str, style: ScalarStyle, tag: Option<&Tag>) -> Result<Value, YamlError> {
    let kind = match tag {
        None if style == ScalarStyle::Plain => implicit_kind(text),
        None => "str",
        Some(tag) if tag.handle.is_empty() && tag.suffix == "!" => "str",
        Some(tag) if tag.is_yaml_core_schema() => tag.suffix.as_str(),
        Some(tag) => return error(format!("unsupported tag {tag}")),
    };
    match kind {
        "str" => Ok(Value::Str(text.to_owned())),
        "null" => Ok(Value::Null),
        "bool" => construct_bool(text),
        "int" => construct_int(text).map(Value::Int),
        "float" => construct_float(text).map(Value::Float),
        other => error(format!("unsupported YAML type !!{other}")),
    }
}

/// The tag PyYAML's implicit resolvers give a plain scalar (`yaml/resolver.py`).
fn implicit_kind(text: &str) -> &'static str {
    static RESOLVERS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    let resolvers = RESOLVERS.get_or_init(|| {
        [
            ("bool", r"^(?:yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF)$"),
            (
                "float",
                r"^(?:[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$",
            ),
            (
                "int",
                r"^(?:[-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+)$",
            ),
            ("merge", r"^(?:<<)$"),
            ("null", r"^(?:~|null|Null|NULL|)$"),
            (
                "timestamp",
                r"^(?:[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]|[0-9][0-9][0-9][0-9]-[0-9][0-9]?-[0-9][0-9]?(?:[Tt]|[ \t]+)[0-9][0-9]?:[0-9][0-9]:[0-9][0-9](?:\.[0-9]*)?(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9][0-9])?))?)$",
            ),
            ("value", r"^(?:=)$"),
        ]
        .into_iter()
        .map(|(kind, pattern)| (kind, Regex::new(pattern).expect("resolver pattern")))
        .collect()
    });
    let kind = resolvers.iter().find(|(_, regex)| regex.is_match(text));
    match kind {
        // A plain `<<` outside a mapping key is the string PyYAML would refuse to merge; keep it a string.
        Some(("merge", _)) => "str",
        Some((kind, _)) => kind,
        None => "str",
    }
}

fn construct_bool(text: &str) -> Result<Value, YamlError> {
    match text.to_lowercase().as_str() {
        "yes" | "true" | "on" => Ok(Value::Bool(true)),
        "no" | "false" | "off" => Ok(Value::Bool(false)),
        _ => error(format!("invalid boolean {text:?}")),
    }
}

/// `SafeConstructor.construct_yaml_int`.
fn construct_int(text: &str) -> Result<i64, YamlError> {
    let cleaned = text.replace('_', "");
    let (sign, digits) = match cleaned.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, cleaned.strip_prefix('+').unwrap_or(&cleaned)),
    };
    let parsed = if digits == "0" {
        Ok(0)
    } else if let Some(binary) = digits.strip_prefix("0b") {
        i64::from_str_radix(binary, 2)
    } else if let Some(hex) = digits.strip_prefix("0x") {
        i64::from_str_radix(hex, 16)
    } else if digits.starts_with('0') {
        i64::from_str_radix(digits, 8)
    } else if digits.contains(':') {
        sexagesimal(digits)
    } else {
        digits.parse()
    };
    match parsed {
        Ok(value) => Ok(sign * value),
        Err(e) => error(format!("invalid or out-of-range integer {text:?}: {e}")),
    }
}

fn sexagesimal(digits: &str) -> Result<i64, std::num::ParseIntError> {
    digits
        .split(':')
        .try_fold(0i64, |total, part| Ok(total * 60 + part.parse::<i64>()?))
}

/// `SafeConstructor.construct_yaml_float`.
fn construct_float(text: &str) -> Result<f64, YamlError> {
    let cleaned = text.replace('_', "").to_lowercase();
    let (sign, body) = match cleaned.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, cleaned.strip_prefix('+').unwrap_or(&cleaned)),
    };
    let value = match body {
        ".inf" => f64::INFINITY,
        ".nan" => f64::NAN,
        _ if body.contains(':') => body
            .split(':')
            .try_fold(0.0, |total, part| {
                part.parse::<f64>().map(|p| total * 60.0 + p)
            })
            .map_err(|e| YamlError(format!("invalid float {text:?}: {e}")))?,
        _ => body
            .parse()
            .map_err(|e| YamlError(format!("invalid float {text:?}: {e}")))?,
    };
    Ok(sign * value)
}

/// `repr(value)` for a value PyYAML built.
pub fn value_repr(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => float_repr(*x),
        Value::Str(text) => repr(text),
        Value::List(items) => format!("[{}]", join_repr(items.iter().map(value_repr))),
        Value::Dict(pairs) => format!(
            "{{{}}}",
            join_repr(
                pairs
                    .iter()
                    .map(|(k, v)| format!("{}: {}", value_repr(k), value_repr(v)))
            )
        ),
        // Python orders a set by hash, which varies per process for strings; this keeps the document's order.
        Value::Set(items) if items.is_empty() => "set()".into(),
        Value::Set(items) => format!("{{{}}}", join_repr(items.iter().map(value_repr))),
    }
}

fn join_repr(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_print_like_python() {
        let list = Value::List(vec![
            Value::Str("a".into()),
            Value::Null,
            Value::Int(3),
            Value::Bool(true),
        ]);
        assert_eq!(value_repr(&list), "['a', None, 3, True]");
        assert_eq!(
            value_repr(&Value::Set(vec![Value::Str(".git".into())])),
            "{'.git'}"
        );
        assert_eq!(value_repr(&Value::Float(1.0)), "1.0");
        assert_eq!(value_repr(&Value::Float(1e300)), "1e+300");
    }

    fn one(text: &str) -> Value {
        load(text).expect("loads").expect("a document")
    }

    #[test]
    fn refusals_do_not_name_a_tool() {
        for text in [
            "<<: {a: 1}\n",
            "a: !custom 1\n",
            "a: !!binary x\n",
            "!custom {a: 1}\n",
        ] {
            let message = load(text).expect_err(text).to_string();
            assert!(!message.contains("gdlint"), "{message}");
        }
    }

    #[test]
    fn plain_scalars_resolve_as_yaml_1_1() {
        let value =
            one("[yes, Off, 017, 1_000, 1:30, 0x1F, 0b11, ~, '', 1.5, 09, -0, .inf, abc, '1']");
        let Value::List(items) = value else {
            panic!("a list")
        };
        assert_eq!(
            items[..9],
            [
                Value::Bool(true),
                Value::Bool(false),
                Value::Int(15),
                Value::Int(1000),
                Value::Int(90),
                Value::Int(31),
                Value::Int(3),
                Value::Null,
                Value::Str(String::new()),
            ]
        );
        assert_eq!(items[9], Value::Float(1.5));
        assert_eq!(items[10], Value::Str("09".into()));
        assert_eq!(items[11], Value::Int(0));
        assert_eq!(items[12], Value::Float(f64::INFINITY));
        assert_eq!(
            items[13..],
            [Value::Str("abc".into()), Value::Str("1".into())]
        );
    }

    #[test]
    fn set_tag_keeps_keys_in_order_and_repeated_keys_keep_first_position() {
        assert_eq!(
            one("a: !!set\n  .git: null\n  .godot:\nb: 1\na: 2\n"),
            Value::Dict(vec![
                (Value::Str("a".into()), Value::Int(2)),
                (Value::Str("b".into()), Value::Int(1))
            ])
        );
        assert_eq!(
            one("!!set {x, y}"),
            Value::Set(vec![Value::Str("x".into()), Value::Str("y".into())])
        );
    }

    #[test]
    fn empty_stream_is_none_and_two_documents_fail() {
        assert_eq!(load("").unwrap(), None);
        assert_eq!(load("# only a comment\n").unwrap(), None);
        assert!(load("a: 1\n---\nb: 2\n").is_err());
    }

    #[test]
    fn unsupported_constructs_fail_loudly() {
        assert!(load("a: !!python/name:os.system x").is_err());
        assert!(load("a: 2001-12-14").is_err());
        assert!(load("base: &b {x: 1}\nc:\n  <<: *b\n").is_err());
    }
}
