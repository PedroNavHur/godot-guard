//! JSON text as Python's `json.dumps(value, indent=2)` writes it, so `check --json` and the baseline
//! file keep 0.4.0's bytes: ASCII only, `": "` between key and value, empty containers as `[]` and `{}`.
//! `dumps_compact` is `json.dumps(value)`, for `complexity --json`.

use std::fmt::Write;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Int(i64),
    Str(String),
    List(Vec<Json>),
    /// Keys in output order; `dumps_sorted` sorts them.
    Object(Vec<(String, Json)>),
}

/// `json.dumps(value, indent=2)`.
pub fn dumps(value: &Json) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0, false);
    out
}

/// `json.dumps(value, indent=2, sort_keys=True)`.
pub fn dumps_sorted(value: &Json) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0, true);
    out
}

/// `json.dumps(value)`: one line, `", "` between items and `": "` between key and value.
pub fn dumps_compact(value: &Json) -> String {
    let mut out = String::new();
    write_compact(&mut out, value);
    out
}

/// `json.dumps(text)`: a quoted string with Python's ASCII escapes.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    write_string(&mut out, text);
    out
}

fn write_value(out: &mut String, value: &Json, depth: usize, sort: bool) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Int(number) => write!(out, "{number}").expect("writing to a String"),
        Json::Str(text) => write_string(out, text),
        Json::List(items) => write_list(out, items, depth, sort),
        Json::Object(entries) => write_object(out, entries, depth, sort),
    }
}

fn write_list(out: &mut String, items: &[Json], depth: usize, sort: bool) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push('[');
    for (index, item) in items.iter().enumerate() {
        separate(out, index, depth + 1);
        write_value(out, item, depth + 1, sort);
    }
    close(out, depth, ']');
}

fn write_object(out: &mut String, entries: &[(String, Json)], depth: usize, sort: bool) {
    if entries.is_empty() {
        out.push_str("{}");
        return;
    }
    let mut ordered: Vec<&(String, Json)> = entries.iter().collect();
    if sort {
        ordered.sort_by(|a, b| a.0.cmp(&b.0));
    }
    out.push('{');
    for (index, (key, item)) in ordered.into_iter().enumerate() {
        separate(out, index, depth + 1);
        write_string(out, key);
        out.push_str(": ");
        write_value(out, item, depth + 1, sort);
    }
    close(out, depth, '}');
}

fn write_compact(out: &mut String, value: &Json) {
    match value {
        Json::List(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_compact(out, item);
            }
            out.push(']');
        }
        Json::Object(entries) => {
            out.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                write_string(out, key);
                out.push_str(": ");
                write_compact(out, item);
            }
            out.push('}');
        }
        scalar => write_value(out, scalar, 0, false),
    }
}

fn separate(out: &mut String, index: usize, depth: usize) {
    if index > 0 {
        out.push(',');
    }
    out.push('\n');
    indent(out, depth);
}

fn close(out: &mut String, depth: usize, bracket: char) {
    out.push('\n');
    indent(out, depth);
    out.push(bracket);
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// Python's `ensure_ascii` escaping: everything outside printable ASCII is escaped, as UTF-16 units.
fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            ' '..='~' => out.push(c),
            _ => {
                let mut units = [0u16; 2];
                for unit in c.encode_utf16(&mut units) {
                    write!(out, "\\u{unit:04x}").expect("writing to a String");
                }
            }
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indents_like_python() {
        let value = Json::Object(vec![
            (
                "b".into(),
                Json::List(vec![Json::Int(1), Json::Str("x".into())]),
            ),
            ("a".into(), Json::Object(vec![])),
            ("c".into(), Json::List(vec![])),
        ]);
        assert_eq!(
            dumps(&value),
            "{\n  \"b\": [\n    1,\n    \"x\"\n  ],\n  \"a\": {},\n  \"c\": []\n}"
        );
        assert!(dumps_sorted(&value).starts_with("{\n  \"a\": {},"));
    }

    #[test]
    fn compacts_like_python() {
        let value = Json::Object(vec![
            (
                "b".into(),
                Json::List(vec![Json::Int(1), Json::Null, Json::Str("é".into())]),
            ),
            ("a".into(), Json::Object(vec![])),
            ("c".into(), Json::List(vec![])),
        ]);
        assert_eq!(
            dumps_compact(&value),
            "{\"b\": [1, null, \"\\u00e9\"], \"a\": {}, \"c\": []}"
        );
    }

    #[test]
    fn escapes_like_python() {
        assert_eq!(
            quote("é\u{1F600}\x7f\"\\\x01"),
            "\"\\u00e9\\ud83d\\ude00\\u007f\\\"\\\\\\u0001\""
        );
    }
}
