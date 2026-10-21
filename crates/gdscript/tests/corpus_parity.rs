//! Differential test against gdtoolkit: every `.gd` file in the corpus parses to the oracle's trees, or is
//! rejected at the oracle's line. Reads the corpus from GG_CORPUS and skips when it is unset; runs
//! `tools/oracle/dump_tree.py` with the corpus's oracle venv (or GG_ORACLE_PYTHON). GG_PARITY_DIRS, a
//! `:`-separated list, replaces the corpus directories, for mutants from `tools/gdscript/mutate_corpus.py`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;

use gdscript::oracle::{Dump, read_line};

mod common;

const CORPUS_DIRS: [&str; 3] = ["hexstead", "wingmen", "gdtoolkit-src/tests"];
const ORACLE_PROCESSES: usize = 4;

#[test]
fn corpus_matches_gdtoolkit() {
    let Ok(corpus) = std::env::var("GG_CORPUS") else {
        eprintln!("GG_CORPUS is unset; skipping the corpus parity test");
        return;
    };
    let corpus = PathBuf::from(corpus);
    let dirs: Vec<PathBuf> = match std::env::var("GG_PARITY_DIRS") {
        Ok(dirs) => dirs.split(':').map(PathBuf::from).collect(),
        Err(_) => CORPUS_DIRS.iter().map(|dir| corpus.join(dir)).collect(),
    };
    let mut files = Vec::new();
    for dir in &dirs {
        collect_gd(dir, &mut files);
    }
    files.sort();
    assert!(!files.is_empty(), "no .gd files under {}", corpus.display());

    let dumps = oracle_dumps(&corpus, &files);
    let mut failures = Vec::new();
    let (mut parsed, mut rejected) = (0, 0);
    for dump in &dumps {
        match common::compare(dump, Path::new("/")) {
            Ok(true) => parsed += 1,
            Ok(false) => rejected += 1,
            Err(failure) => failures.push(failure),
        }
    }
    eprintln!(
        "corpus parity: {} files, {parsed} parsed and {rejected} rejected identically, {} differing",
        dumps.len(),
        failures.len()
    );
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn collect_gd(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_gd(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "gd") {
            files.push(path);
        }
    }
}

/// The oracle's dump of every file, in `files` order, from a few dumper processes run side by side.
fn oracle_dumps(corpus: &Path, files: &[PathBuf]) -> Vec<Dump> {
    let python = std::env::var("GG_ORACLE_PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|_| corpus.join("oracle-venv/bin/python"));
    let dumper = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/oracle/dump_tree.py");
    let chunk = files.len().div_ceil(ORACLE_PROCESSES);
    let outputs: Vec<String> = thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|chunk| {
                let (python, dumper) = (&python, &dumper);
                scope.spawn(move || {
                    let output = Command::new(python)
                        .arg("-I")
                        .arg(dumper)
                        .args(chunk)
                        .output()
                        .expect("run the oracle dumper");
                    assert!(
                        output.status.success(),
                        "dumper failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    String::from_utf8(output.stdout).expect("dumper prints UTF-8")
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("dumper thread"))
            .collect()
    });
    let dumps: Vec<Dump> = outputs
        .iter()
        .flat_map(|out| out.lines())
        .map(|line| read_line(line).expect("load dump line"))
        .collect();
    assert_eq!(dumps.len(), files.len(), "one dump line per file");
    dumps
}
