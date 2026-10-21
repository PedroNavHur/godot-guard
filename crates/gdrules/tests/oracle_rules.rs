//! Differential tests: the Rust rules against godot-guard 0.4.0's on gdtoolkit's own trees.
//!
//! `fixtures_match_the_oracle` reads the committed `tests/data/fixtures.jsonl`. Regenerate it from this
//! directory after changing a fixture:
//!     $GG_CORPUS/oracle-venv/bin/python -I ../../tools/oracle/dump_rules.py tests/fixtures/*.gd \
//!         > tests/data/fixtures.jsonl
//! `corpus_matches_the_oracle` runs the oracle over every script in hexstead, wingmen, and gdtoolkit's
//! valid scripts under `GG_CORPUS`, and skips when it is unset. Those scripts are clean on the pattern
//! rules, so `corpus_variants_match_the_oracle` also runs both sides on rewritten copies that trip them.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use common::{Tally, read_line};

const CORPUS_ROOTS: [&str; 3] = [
    "hexstead",
    "wingmen",
    "gdtoolkit-src/tests/valid-gd-scripts",
];

fn assert_parity(tally: &Tally) {
    eprintln!("gdrules parity: {}", tally.summary());
    assert!(
        tally.differing.is_empty(),
        "{}\n{}",
        tally.summary(),
        tally.differing.join("\n")
    );
}

#[test]
fn fixtures_match_the_oracle() {
    let dump = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/fixtures.jsonl");
    let mut tally = Tally::default();
    for line in std::fs::read_to_string(dump)
        .expect("read fixture dump")
        .lines()
    {
        tally.add(&read_line(line));
    }
    assert!(tally.compared > 0);
    assert_parity(&tally);
}

fn scripts(dir: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read corpus directory")
        .map(|e| e.expect("read entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() && path.file_name().is_some_and(|name| name != ".godot") {
            scripts(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "gd") {
            found.push(path);
        }
    }
}

fn corpus() -> Option<(PathBuf, Vec<PathBuf>)> {
    let Some(corpus) = std::env::var_os("GG_CORPUS").map(PathBuf::from) else {
        eprintln!("GG_CORPUS is unset: skipping the corpus parity run");
        return None;
    };
    let mut files = Vec::new();
    for root in CORPUS_ROOTS {
        scripts(&corpus.join(root), &mut files);
    }
    Some((corpus, files))
}

/// Run the oracle on `files` and compare every line with the Rust rules.
fn run_oracle(corpus: &Path, files: &[PathBuf]) -> Tally {
    let oracle = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/oracle/dump_rules.py");
    let output = Command::new(corpus.join("oracle-venv/bin/python"))
        .arg("-I")
        .arg(oracle)
        .args(files)
        .output()
        .expect("run the rules oracle");
    assert!(
        output.status.success(),
        "oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("oracle output is UTF-8");
    let mut tally = Tally::default();
    for line in stdout.lines() {
        tally.add(&read_line(line));
    }
    assert_eq!(
        tally.compared + tally.rejected,
        files.len(),
        "one oracle line per script"
    );
    tally
}

#[test]
fn corpus_matches_the_oracle() {
    let Some((corpus, files)) = corpus() else {
        return;
    };
    assert_parity(&run_oracle(&corpus, &files));
}

/// `source` rewritten to trip the pattern rules: every function a `_process`, every `@export var` private,
/// and every plain `target = value` statement a self-assignment.
fn variant(source: &str) -> String {
    let mut lines = Vec::new();
    for line in source.lines() {
        let code = line.trim_start_matches('\t');
        let indent = &line[..line.len() - code.len()];
        let rewritten =
            if let Some(rest) = code.strip_prefix("func ").filter(|rest| rest.contains('(')) {
                format!(
                    "{indent}func _process{}",
                    &rest[rest.find('(').unwrap_or(0)..]
                )
            } else if let Some((target, _)) = code
                .split_once(" = ")
                .filter(|(target, _)| is_plain_target(target))
            {
                format!("{indent}{target} = {target}")
            } else {
                line.replace("@export var ", "@export var _")
            };
        lines.push(rewritten);
    }
    lines.join("\n") + "\n"
}

fn is_plain_target(target: &str) -> bool {
    !target.is_empty() && !target.contains([' ', '(', '"', '\'', '#']) && !target.starts_with('@')
}

#[test]
fn corpus_variants_match_the_oracle() {
    let Some((corpus, files)) = corpus() else {
        return;
    };
    let out = std::env::temp_dir().join(format!("gdrules-variants-{}", std::process::id()));
    let mut variants = Vec::new();
    for (index, file) in files.iter().enumerate() {
        let source = std::fs::read_to_string(file).expect("read corpus script");
        let name = file
            .file_name()
            .expect("script has a name")
            .to_string_lossy();
        let path = out.join(format!("{index:04}-{name}"));
        std::fs::create_dir_all(&out).expect("create variant directory");
        std::fs::write(&path, variant(&source)).expect("write variant");
        variants.push(path);
    }
    let tally = run_oracle(&corpus, &variants);
    std::fs::remove_dir_all(&out).expect("remove variant directory");
    eprintln!("gdrules variant hits: {:?}", tally.hits);
    assert!(
        tally.hits.values().all(|&count| count > 0),
        "a pattern rule never fires: {:?}",
        tally.hits
    );
    assert_parity(&tally);
}
