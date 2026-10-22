//! `godot-guard complexity --json`: every function's scores in the given files, as pedro-best-practices'
//! `complexity_gdscript.py` prints them (`{path: [function] | {"error": message}}`), so it can read them
//! from here. No config: the files are exactly the ones given, scored in parallel, printed in order.

use std::path::Path;

use gdrules::complexity::{self, Function};
use gdscript::{ErrorKind, ParseError};
use pycompat::builtins::{self, ReadError};
use rayon::prelude::*;

use crate::json::{self, Json};

/// The JSON object for `paths`: keyed as given, in order; a path given twice keeps its first place.
pub fn report(paths: &[String]) -> String {
    let mut unique: Vec<&String> = Vec::with_capacity(paths.len());
    for path in paths {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    let entries = unique
        .into_par_iter()
        .map(|path| (path.clone(), scores(path)))
        .collect();
    json::dumps_compact(&Json::Object(entries))
}

/// The file's functions, or `{"error": message}` when it doesn't read or parse.
fn scores(path: &str) -> Json {
    let source = match builtins::read_text(Path::new(path)) {
        Ok(source) => source,
        Err(error) => return failure(read_message(&error)),
    };
    match gdscript::parse(&source) {
        Ok(tree) => Json::List(complexity::score(&tree).iter().map(function).collect()),
        Err(error) => failure(parse_message(&error)),
    }
}

fn function(function: &Function) -> Json {
    let count = |value: usize| Json::Int(value.try_into().expect("a score fits in i64"));
    Json::Object(vec![
        ("name".into(), Json::Str(function.name.clone())),
        ("line".into(), count(function.line)),
        ("end_line".into(), count(function.end_line)),
        ("cyclomatic".into(), count(function.cyclomatic)),
        (
            "cognitive".into(),
            function.cognitive.map_or(Json::Null, count),
        ),
    ])
}

fn failure(message: String) -> Json {
    Json::Object(vec![("error".into(), Json::Str(message))])
}

/// The Python exception's type and the first line of its text, as the Python analyzer reports it.
fn parse_message(error: &ParseError) -> String {
    let kind = match &error.kind {
        ErrorKind::UnexpectedToken { .. } => "UnexpectedToken",
        ErrorKind::UnexpectedCharacters { .. } => "UnexpectedCharacters",
        ErrorKind::Dedent { .. } => "DedentError",
        ErrorKind::Indenter(what) => return (*what).to_owned(),
    };
    let text = error.to_string();
    format!("{kind}: {}", text.lines().next().unwrap_or_default())
}

fn read_message(error: &ReadError) -> String {
    match error {
        ReadError::Io(error) => format!("OSError: {error}"),
        ReadError::Utf8(error) => format!("UnicodeDecodeError: {error}"),
    }
}
