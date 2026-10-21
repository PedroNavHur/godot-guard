//! A `gdlint`-compatible command, for the parity harness and for running the port by hand.
//!
//! Until `gdscript::parse` exists, trees come from an oracle dump: set `GDLINT_ORACLE_TREES` to the output of
//! `tools/oracle/dump_tree.py`, run from the same directory, for the files to lint.

use std::collections::HashMap;
use std::io::Write;
use std::process::ExitCode;

use gdscript::Tree;
use gdscript::oracle::{Dump, read_line};

/// Each dumped file's text, as gdlint reads it, mapped to its tree or to the parse error gdtoolkit raised.
fn oracle_trees(dump: &str) -> Result<HashMap<String, Result<Tree, String>>, String> {
    let text = std::fs::read_to_string(dump).map_err(|e| format!("cannot read {dump}: {e}"))?;
    let mut trees = HashMap::new();
    for line in text.lines() {
        let (path, outcome) = match read_line(line)? {
            Dump::Parsed { path, tree, .. } => (path, Ok(tree)),
            Dump::Rejected { path, error } => (
                path,
                Err(error["message"].as_str().unwrap_or_default().to_owned()),
            ),
        };
        let source = gdlint::read_source(path.as_ref())
            .map_err(|e| format!("cannot read {path}: {e}"))?
            .map_err(|e| format!("{path} is not UTF-8: {e}"))?;
        trees.insert(source, outcome);
    }
    Ok(trees)
}

fn main() -> ExitCode {
    let fail = |message: String| {
        eprintln!("gdlint: {message}");
        ExitCode::from(2)
    };
    let Ok(dump) = std::env::var("GDLINT_ORACLE_TREES") else {
        return fail("set GDLINT_ORACLE_TREES to a tools/oracle/dump_tree.py dump; the parser is not ported yet".into());
    };
    let trees = match oracle_trees(&dump) {
        Ok(trees) => trees,
        Err(message) => return fail(message),
    };
    let parse = |source: &str| match trees.get(source) {
        Some(outcome) => outcome.clone(),
        None => panic!("no oracle tree for a file with this text:\n{source}"),
    };
    let cwd = std::env::current_dir().expect("the working directory");
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (mut out, mut err) = (std::io::stdout().lock(), std::io::stderr().lock());
    let status = gdlint::cli::run(&argv, &cwd, &parse, &mut out, &mut err).expect("write output");
    out.flush().expect("flush stdout");
    ExitCode::from(status as u8)
}
