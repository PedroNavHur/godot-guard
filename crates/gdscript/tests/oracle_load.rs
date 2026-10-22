//! Every oracle dump line loads, when GG_ORACLE_DUMP names a dump file.

#![cfg(feature = "oracle")]

use gdscript::oracle::{Dump, read_line};

#[test]
fn oracle_dump_loads() {
    let Ok(path) = std::env::var("GG_ORACLE_DUMP") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("read dump");
    let mut parsed = 0;
    for line in text.lines() {
        if let Dump::Parsed { tree, .. } = read_line(line).expect("load dump line") {
            assert!(tree.iter_topdown().count() > 0);
            parsed += 1;
        }
    }
    assert!(parsed > 0);
}
