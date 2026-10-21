//! Differential test against gdtoolkit 4.5.0: every case `tools/oracle/gdformat_cases.py` emits for the
//! corpus under GG_CORPUS (hexstead, wingmen, gdtoolkit's own test scripts, and perturbed variants of
//! each) must format byte for byte as the oracle did, with the same safety verdict, both on the oracle's
//! trees and on the trees `gdscript` parses. Skips when GG_CORPUS is unset.
//!
//! Mismatches are written under `$CARGO_TARGET_TMPDIR/gdformat-mismatches/` as `.expected`/`.actual`
//! pairs, so `diff` shows each one.

mod cases;

use std::collections::BTreeMap;

use cases::{Verdict, compare, compare_parsed, corpus_cases};

#[derive(Default, Debug)]
struct Tally {
    identical: usize,
    safety_checked: usize,
    differing: usize,
    both_crashed: usize,
    rejected: usize,
}

/// Deep expression trees recurse deeper than a test thread's default stack allows.
const STACK_BYTES: usize = 512 << 20;

#[test]
fn formats_like_gdtoolkit() {
    std::thread::Builder::new()
        .stack_size(STACK_BYTES)
        .spawn(compare_corpus)
        .unwrap()
        .join()
        .unwrap();
}

fn compare_corpus() {
    let Some(cases) = corpus_cases() else {
        eprintln!("GG_CORPUS unset; skipping the gdformat parity run");
        return;
    };
    let mismatches = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("gdformat-mismatches");
    let _ = std::fs::remove_dir_all(&mismatches);
    std::fs::create_dir_all(&mismatches).unwrap();
    std::panic::set_hook(Box::new(|_| {}));

    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    let mut failures = Vec::new();
    for case in cases {
        let (id, variant) = (case.id.clone(), case.variant.clone());
        let parsed = compare_parsed(&case);
        for (trees, verdict) in [("parser", parsed), ("oracle", compare(case))] {
            let tally = tallies
                .entry(format!("{variant}, {trees} trees"))
                .or_default();
            record(
                tally,
                verdict,
                &format!("{id}#{trees}"),
                &mismatches,
                &mut failures,
            );
        }
    }
    let _ = std::panic::take_hook();
    for (variant, tally) in &tallies {
        eprintln!("{variant:>24}: {tally:?}");
    }
    let compared: usize = tallies
        .values()
        .map(|t| t.identical + t.differing + t.both_crashed)
        .sum();
    eprintln!("{compared} cases compared, {} differing", failures.len());
    assert!(
        failures.is_empty(),
        "{} cases differ from gdtoolkit (see {}): {:?}",
        failures.len(),
        mismatches.display(),
        &failures[..failures.len().min(20)]
    );
}

fn record(
    tally: &mut Tally,
    verdict: Verdict,
    id: &str,
    mismatches: &std::path::Path,
    failures: &mut Vec<String>,
) {
    match verdict {
        Verdict::Identical { safety_checked } => {
            tally.identical += 1;
            tally.safety_checked += usize::from(safety_checked);
        }
        Verdict::BothCrashed => tally.both_crashed += 1,
        Verdict::Rejected => tally.rejected += 1,
        Verdict::Differs { expected, actual } => {
            tally.differing += 1;
            let name = id.replace(['/', '#'], "_");
            std::fs::write(mismatches.join(format!("{name}.expected")), expected).unwrap();
            std::fs::write(mismatches.join(format!("{name}.actual")), actual).unwrap();
            failures.push(id.to_owned());
        }
    }
}
