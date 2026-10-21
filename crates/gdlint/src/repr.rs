//! Python's `repr`, for the text gdlint prints through `logging` and docopt prints in its usage errors.

use crate::yaml::Value;

/// `repr(text)` for a `str`.
pub fn str_repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if is_printable(c) => out.push(c),
            c if (c as u32) < 0x100 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if (c as u32) < 0x10000 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push_str(&format!("\\U{:08x}", c as u32)),
        }
    }
    out.push(quote);
    out
}

/// Approximates `str.isprintable`: control and separator characters other than the space are escaped.
// ceiling: Python consults the full Unicode category table (Cc, Cf, Cs, Co, Cn, Zl, Zp, Zs); this covers the
// characters a config or a command line plausibly holds. Port the table if verbose output ever needs it.
fn is_printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    !(c.is_control()
        || c.is_whitespace()
        || matches!(c, '\u{ad}' | '\u{200b}'..='\u{200f}' | '\u{feff}'))
}

/// `repr(value)` for a value PyYAML built.
pub fn value_repr(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Int(n) => n.to_string(),
        Value::Float(x) => float_repr(*x),
        Value::Str(text) => str_repr(text),
        Value::List(items) => format!("[{}]", join(items.iter().map(value_repr))),
        Value::Dict(pairs) => format!(
            "{{{}}}",
            join(
                pairs
                    .iter()
                    .map(|(k, v)| format!("{}: {}", value_repr(k), value_repr(v)))
            )
        ),
        // Python orders a set by hash, which varies per process for strings; this keeps the document's order.
        Value::Set(items) if items.is_empty() => "set()".into(),
        Value::Set(items) => format!("{{{}}}", join(items.iter().map(value_repr))),
    }
}

fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let text = format!("{x:?}");
    // Rust writes 1e300 where Python writes 1e+300.
    match text.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') => {
            format!("{mantissa}e+{exponent}")
        }
        _ => text,
    }
}

fn join(parts: impl Iterator<Item = String>) -> String {
    parts.collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_quote_like_python() {
        assert_eq!(str_repr("abc"), "'abc'");
        assert_eq!(str_repr("it's"), "\"it's\"");
        assert_eq!(str_repr("both ' and \""), "'both \\' and \"'");
        assert_eq!(str_repr("a\\b\n\u{1}"), "'a\\\\b\\n\\x01'");
        assert_eq!(str_repr("é"), "'é'");
    }

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
}
