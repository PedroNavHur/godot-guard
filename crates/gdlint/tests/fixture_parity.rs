//! The tricky cases the corpus parity run covers, committed so they run without the corpus: each case in
//! `fixtures/lint/cases.txt` must print what gdtoolkit 4.5.0's gdlint printed, recorded in `<case>.expected`
//! by `tools/oracle/gdlint_fixtures.sh`, linting the oracle's trees in `fixtures/lint/trees`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gdscript::Tree;
use gdscript::oracle::{Dump, read_line};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lint")
}

fn trees() -> HashMap<String, Tree> {
    let mut trees = HashMap::new();
    for entry in std::fs::read_dir(fixtures().join("trees")).expect("list trees") {
        let dump = std::fs::read_to_string(entry.expect("entry").path()).expect("read dump");
        for line in dump.lines() {
            let Dump::Parsed { path, tree, .. } = read_line(line).expect("dump line") else {
                panic!("a fixture the oracle rejects: {line}");
            };
            let source = gdlint::read_source(&fixtures().join(path))
                .expect("read")
                .expect("UTF-8");
            trees.insert(source, tree);
        }
    }
    trees
}

#[test]
fn fixtures_print_what_the_oracle_printed() {
    let trees = trees();
    let parse = |source: &str| {
        Ok(trees
            .get(source)
            .cloned()
            .expect("a tree for every fixture"))
    };
    let cases = std::fs::read_to_string(fixtures().join("cases.txt")).expect("cases.txt");
    for case in cases.lines() {
        let mut words = case.split_whitespace();
        let (name, dir) = (words.next().expect("name"), words.next().expect("dir"));
        let args: Vec<String> = words.map(str::to_owned).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = gdlint::cli::run(&args, &fixtures().join(dir), &parse, &mut out, &mut err)
            .expect("run");
        let actual = format!(
            "exit {status}\n--- stdout\n{}--- stderr\n{}",
            String::from_utf8(out).expect("UTF-8"),
            String::from_utf8(err).expect("UTF-8")
        );
        let expected =
            std::fs::read_to_string(fixtures().join(format!("{name}.expected"))).expect("expected");
        assert_eq!(actual, expected, "case {name}");
    }
}
