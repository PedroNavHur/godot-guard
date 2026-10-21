//! `.godot-guard.yml` as 0.4.0's `StrictLoader` reads it: PyYAML's safe loader (YAML 1.1 scalars, so a
//! bare `off` is false) without anchors, aliases, or merge keys, and with a key repeated in one mapping an
//! error. yaml-rust2 tokenizes; composing, resolving, and constructing follow PyYAML.

use std::collections::VecDeque;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use yaml_rust2::parser::{Event, Parser, Tag};
use yaml_rust2::scanner::{Marker, TScalarStyle};

const TAG_PREFIX: &str = "tag:yaml.org,2002:";
const UNSUPPORTED: &str = "anchors and aliases are not supported";

/// A constructed YAML value, typed as Python sees it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Seq(Vec<Value>),
    Map(Vec<(Value, Value)>),
    /// A timestamp, kept as its text.
    Timestamp(String),
}

impl Value {
    /// `str(value)` in Python.
    pub fn py_str(&self) -> String {
        match self {
            Value::Str(text) | Value::Timestamp(text) => text.clone(),
            _ => self.py_repr(),
        }
    }

    /// `repr(value)` in Python.
    pub fn py_repr(&self) -> String {
        match self {
            Value::Null => "None".into(),
            Value::Bool(true) => "True".into(),
            Value::Bool(false) => "False".into(),
            Value::Int(number) => number.to_string(),
            Value::Float(number) => float_repr(*number),
            Value::Str(text) => crate::py::repr(text),
            Value::Timestamp(text) => format!("datetime({})", crate::py::repr(text)),
            Value::Seq(items) => format!("[{}]", join(items.iter().map(Value::py_repr))),
            Value::Map(entries) => {
                format!(
                    "{{{}}}",
                    join(
                        entries
                            .iter()
                            .map(|(k, v)| format!("{}: {}", k.py_repr(), v.py_repr()))
                    )
                )
            }
        }
    }

    /// Python's `==`, where `True == 1 == 1.0`.
    fn py_eq(&self, other: &Value) -> bool {
        match (self.number(), other.number()) {
            (Some(a), Some(b)) => a == b,
            _ => self == other,
        }
    }

    fn number(&self) -> Option<f64> {
        match *self {
            Value::Bool(flag) => Some(f64::from(u8::from(flag))),
            Value::Int(number) => Some(number as f64),
            Value::Float(number) => Some(number),
            _ => None,
        }
    }
}

fn join(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(", ")
}

/// `repr(float)`: the shortest round-trip digits, in exponent form below 1e-4 and from 1e16.
fn float_repr(number: f64) -> String {
    if number.is_nan() {
        return "nan".into();
    }
    if number.is_infinite() {
        return if number > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let magnitude = number.abs();
    if magnitude != 0.0 && !(1e-4..1e16).contains(&magnitude) {
        let text = format!("{number:e}");
        let (mantissa, exponent) = text.split_once('e').expect("exponent form");
        let exponent: i32 = exponent.parse().expect("an exponent");
        return format!(
            "{mantissa}e{}{:02}",
            if exponent < 0 { '-' } else { '+' },
            exponent.abs()
        );
    }
    let text = format!("{number}");
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

/// A loading error: PyYAML's problem, and the 1-based line it points at.
#[derive(Debug, PartialEq)]
pub struct YamlError {
    pub line: Option<usize>,
    pub problem: String,
}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.problem),
            None => f.write_str(&self.problem),
        }
    }
}

fn error(mark: Marker, problem: impl Into<String>) -> YamlError {
    YamlError {
        line: Some(mark.line()),
        problem: problem.into(),
    }
}

/// A composed node, before construction.
enum Node {
    Scalar {
        text: String,
        tag: String,
        mark: Marker,
    },
    Seq {
        items: Vec<Node>,
        tag: String,
        mark: Marker,
    },
    Map {
        entries: Vec<(Node, Node)>,
        tag: String,
        mark: Marker,
    },
}

/// The document in `text`, or None for an empty stream.
pub fn load(text: &str) -> Result<Option<Value>, YamlError> {
    let mut events = Events {
        parser: Parser::new_from_str(text),
        last_line: text.matches('\n').count() + 1,
    };
    events.next()?; // StreamStart
    let (event, _) = events.next()?;
    if matches!(event, Event::StreamEnd) {
        return Ok(None);
    }
    let first = events.next()?;
    let root = compose(&mut events, first)?;
    events.next()?; // DocumentEnd
    let (event, mark) = events.next()?;
    if !matches!(event, Event::StreamEnd) {
        return Err(error(mark, "but found another document"));
    }
    validate(&root)?;
    Ok(Some(construct(&root)))
}

struct Events<'a> {
    parser: Parser<std::str::Chars<'a>>,
    /// The line PyYAML's end-of-stream mark is on.
    last_line: usize,
}

impl Events<'_> {
    fn next(&mut self) -> Result<(Event, Marker), YamlError> {
        self.parser.next_token().map_err(|e| {
            // yaml-rust2 marks where the unterminated token began; PyYAML marks where the stream ended.
            let line = if e.info().ends_with("unexpected end of stream") {
                self.last_line
            } else {
                e.marker().line()
            };
            // yaml-rust2 rejects an alias to an undefined anchor while parsing; the strict loader refuses
            // every alias before PyYAML would look the anchor up.
            let problem = if e.info().ends_with("found unknown anchor") {
                UNSUPPORTED
            } else {
                e.info()
            };
            YamlError {
                line: Some(line),
                problem: problem.to_owned(),
            }
        })
    }
}

fn compose(events: &mut Events, (event, mark): (Event, Marker)) -> Result<Node, YamlError> {
    let unsupported = || error(mark, UNSUPPORTED);
    match event {
        Event::Alias(_) => Err(unsupported()),
        Event::Scalar(_, _, anchor, _)
        | Event::SequenceStart(anchor, _)
        | Event::MappingStart(anchor, _)
            if anchor != 0 =>
        {
            Err(unsupported())
        }
        Event::Scalar(text, style, _, tag) => {
            let tag = resolve(&text, style, tag);
            Ok(Node::Scalar { text, tag, mark })
        }
        Event::SequenceStart(_, tag) => {
            let mut items = Vec::new();
            loop {
                let next = events.next()?;
                if matches!(next.0, Event::SequenceEnd) {
                    break;
                }
                items.push(compose(events, next)?);
            }
            Ok(Node::Seq {
                items,
                tag: collection_tag(tag, "seq"),
                mark,
            })
        }
        Event::MappingStart(_, tag) => {
            let mut entries = Vec::new();
            loop {
                let next = events.next()?;
                if matches!(next.0, Event::MappingEnd) {
                    break;
                }
                let key = compose(events, next)?;
                let next = events.next()?;
                let value = compose(events, next)?;
                entries.push((key, value));
            }
            Ok(Node::Map {
                entries,
                tag: collection_tag(tag, "map"),
                mark,
            })
        }
        other => Err(error(mark, format!("unexpected {other:?}"))),
    }
}

fn full_tag(tag: &Tag) -> String {
    if tag.handle == "!!" {
        format!("{TAG_PREFIX}{}", tag.suffix)
    } else {
        format!("{}{}", tag.handle, tag.suffix)
    }
}

fn collection_tag(tag: Option<Tag>, kind: &str) -> String {
    match tag.map(|t| full_tag(&t)) {
        Some(tag) if tag != "!" => tag,
        _ => format!("{TAG_PREFIX}{kind}"),
    }
}

/// Resolvers for plain scalars, in PyYAML's registration order; the first match wins.
static IMPLICIT: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
    let float = concat!(
        r"^(?:[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?",
        r"|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$"
    );
    let int = concat!(
        r"^(?:[-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+",
        r"|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+)$"
    );
    let timestamp = concat!(
        r"^(?:[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]|[0-9][0-9][0-9][0-9]-[0-9][0-9]?-[0-9][0-9]?",
        r"(?:[Tt]|[ \t]+)[0-9][0-9]?:[0-9][0-9]:[0-9][0-9](?:\.[0-9]*)?",
        r"(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9][0-9])?))?)$"
    );
    [
        (
            "bool",
            r"^(?:yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF)$",
        ),
        ("float", float),
        ("int", int),
        ("merge", r"^(?:<<)$"),
        ("null", r"^(?:~|null|Null|NULL|)$"),
        ("timestamp", timestamp),
        ("value", r"^(?:=)$"),
    ]
    .into_iter()
    .map(|(kind, pattern)| (kind, Regex::new(pattern).expect("a valid resolver pattern")))
    .collect()
});

fn resolve(text: &str, style: TScalarStyle, tag: Option<Tag>) -> String {
    if let Some(tag) = tag.map(|t| full_tag(&t)).filter(|t| t != "!") {
        return tag;
    }
    let kind = match style {
        TScalarStyle::Plain => IMPLICIT
            .iter()
            .find(|(_, regex)| regex.is_match(text))
            .map_or("str", |(k, _)| k),
        _ => "str",
    };
    format!("{TAG_PREFIX}{kind}")
}

/// The checks construction makes, in PyYAML's order. A collection's scalars are constructed with it, and
/// its collections are deferred until the current level is done; a mapping checks its merge and duplicate
/// keys before any key's hashability.
fn validate(root: &Node) -> Result<(), YamlError> {
    let mut deferred = VecDeque::new();
    visit(root, &mut deferred)?;
    while let Some(node) = deferred.pop_front() {
        match node {
            Node::Scalar { .. } => {}
            Node::Seq { items, .. } => {
                for item in items {
                    visit(item, &mut deferred)?;
                }
            }
            Node::Map { entries, .. } => {
                check_keys(entries)?;
                for (key, value) in entries {
                    if !matches!(key, Node::Scalar { .. }) {
                        return Err(error(mark_of(key), "found unhashable key"));
                    }
                    visit(value, &mut deferred)?;
                }
            }
        }
    }
    Ok(())
}

/// Construct a scalar now, or defer a collection.
fn visit<'a>(node: &'a Node, deferred: &mut VecDeque<&'a Node>) -> Result<(), YamlError> {
    match node {
        Node::Scalar { text, tag, mark } => scalar(text, tag)
            .map(drop)
            .map_err(|problem| error(*mark, problem)),
        Node::Seq { tag, mark, .. } | Node::Map { tag, mark, .. } => {
            let kind = if matches!(node, Node::Seq { .. }) {
                "seq"
            } else {
                "map"
            };
            if *tag != format!("{TAG_PREFIX}{kind}") {
                // ceiling: PyYAML says "expected a mapping node, but found sequence" for a known tag on
                // the wrong kind of node; any foreign tag on a collection gets the unknown-tag message.
                return Err(error(*mark, unknown_tag(tag)));
            }
            deferred.push_back(node);
            Ok(())
        }
    }
}

fn check_keys(entries: &[(Node, Node)]) -> Result<(), YamlError> {
    let mut seen: Vec<Value> = Vec::new();
    for (key, _) in entries {
        if let Node::Scalar { tag, mark, .. } = key
            && tag == &format!("{TAG_PREFIX}merge")
        {
            return Err(error(*mark, "merge keys are not supported"));
        }
        // A collection key is still its empty placeholder when PyYAML compares it.
        let value = match key {
            Node::Scalar { text, tag, mark } => {
                scalar(text, tag).map_err(|problem| error(*mark, problem))?
            }
            Node::Seq { .. } => Value::Seq(Vec::new()),
            Node::Map { .. } => Value::Map(Vec::new()),
        };
        if seen.iter().any(|earlier| earlier.py_eq(&value)) {
            return Err(error(
                mark_of(key),
                format!("duplicate key {}", value.py_repr()),
            ));
        }
        seen.push(value);
    }
    Ok(())
}

fn mark_of(node: &Node) -> Marker {
    match node {
        Node::Scalar { mark, .. } | Node::Seq { mark, .. } | Node::Map { mark, .. } => *mark,
    }
}

/// Build the value of a validated tree.
fn construct(node: &Node) -> Value {
    match node {
        Node::Scalar { text, tag, .. } => scalar(text, tag).expect("validated"),
        Node::Seq { items, .. } => Value::Seq(items.iter().map(construct).collect()),
        Node::Map { entries, .. } => Value::Map(
            entries
                .iter()
                .map(|(k, v)| (construct(k), construct(v)))
                .collect(),
        ),
    }
}

fn unknown_tag(tag: &str) -> String {
    format!(
        "could not determine a constructor for the tag {}",
        crate::py::repr(tag)
    )
}

fn scalar(text: &str, tag: &str) -> Result<Value, String> {
    let unknown = || unknown_tag(tag);
    match tag.strip_prefix(TAG_PREFIX).ok_or_else(unknown)? {
        "str" => Ok(Value::Str(text.to_owned())),
        "null" => Ok(Value::Null),
        "bool" => Ok(Value::Bool(matches!(
            text.to_lowercase().as_str(),
            "yes" | "true" | "on"
        ))),
        "int" => int(text).ok_or_else(|| format!("invalid int {}", crate::py::repr(text))),
        "float" => float(text).ok_or_else(|| format!("invalid float {}", crate::py::repr(text))),
        "timestamp" => Ok(Value::Timestamp(text.to_owned())),
        _ => Err(unknown()),
    }
}

/// `construct_yaml_int`; None where Python would raise or the value overflows.
fn int(text: &str) -> Option<Value> {
    let digits = text.replace('_', "");
    let (sign, body) = match digits.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, digits.strip_prefix('+').unwrap_or(&digits)),
    };
    let magnitude = if body == "0" {
        0
    } else if let Some(binary) = body.strip_prefix("0b") {
        i64::from_str_radix(binary, 2).ok()?
    } else if let Some(hex) = body.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()?
    } else if body.starts_with('0') {
        i64::from_str_radix(body, 8).ok()?
    } else if body.contains(':') {
        body.split(':').try_fold(0i64, |total, part| {
            total.checked_mul(60)?.checked_add(part.parse().ok()?)
        })?
    } else {
        body.parse().ok()?
    };
    Some(Value::Int(sign * magnitude))
}

/// `construct_yaml_float`.
fn float(text: &str) -> Option<Value> {
    let lowered = text.replace('_', "").to_lowercase();
    let (sign, body) = match lowered.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, lowered.strip_prefix('+').unwrap_or(&lowered)),
    };
    let magnitude = match body {
        ".inf" => f64::INFINITY,
        ".nan" => f64::NAN,
        _ if body.contains(':') => body.split(':').try_fold(0.0, |total, part| {
            Some(total * 60.0 + part.parse::<f64>().ok()?)
        })?,
        _ => body.parse().ok()?,
    };
    Some(Value::Float(sign * magnitude))
}
