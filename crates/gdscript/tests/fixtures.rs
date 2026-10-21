//! The tricky cases the corpus parity runs found, checked without the corpus: each file in
//! `tests/fixtures` against gdtoolkit's dump of it. Regenerate the dump after adding a fixture, from
//! `tests/fixtures`: `~/.cache/gg-corpus/oracle-venv/bin/python -I ../../../../tools/oracle/dump_tree.py *.gd > oracle.jsonl`

use std::path::Path;

use gdscript::oracle::read_line;

mod common;

#[test]
fn fixtures_match_gdtoolkit() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dump = std::fs::read_to_string(dir.join("oracle.jsonl")).expect("read oracle.jsonl");
    let gd_files = std::fs::read_dir(&dir)
        .expect("read fixtures")
        .filter(|entry| {
            entry
                .as_ref()
                .expect("entry")
                .path()
                .extension()
                .is_some_and(|e| e == "gd")
        })
        .count();
    let failures: Vec<String> = dump
        .lines()
        .map(|line| read_line(line).expect("load dump line"))
        .filter_map(|dump| common::compare(&dump, &dir).err())
        .collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    assert_eq!(
        dump.lines().count(),
        gd_files,
        "every fixture is in oracle.jsonl"
    );
}
