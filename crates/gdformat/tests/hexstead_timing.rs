//! How long the port takes to parse and format every hexstead script, against gdformat's 7.2 s for the
//! same files. Ignored by default; run it with GG_CORPUS set and `--release -- --ignored --nocapture`.
//! Single-threaded, as gdformat is; the safety checks are off, as they are by default.

mod cases;

use std::time::{Duration, Instant};

use gdformat::cli::{Options, format_file_code};
use gdformat::parse_code;

#[test]
#[ignore = "a benchmark: run on demand with GG_CORPUS set"]
fn formats_hexstead_in_time() {
    let Some(corpus) = std::env::var_os("GG_CORPUS") else {
        eprintln!("GG_CORPUS is unset; skipping");
        return;
    };
    let scripts: Vec<String> = cases::corpus_files(corpus.as_ref())
        .into_iter()
        .filter(|path| path.components().any(|part| part.as_os_str() == "hexstead"))
        .map(|path| std::fs::read_to_string(path).unwrap().replace("\r\n", "\n"))
        .collect();
    let options = Options {
        line_length: 100,
        spaces_for_indent: None,
        safety_checks: false,
    };
    let (mut parse_time, mut format_time, mut changed) = (Duration::ZERO, Duration::ZERO, 0);
    for code in &scripts {
        let start = Instant::now();
        let parsed = parse_code(code);
        parse_time += start.elapsed();
        let start = Instant::now();
        let outcome = format_file_code(code, &options, "", parsed.as_ref(), parse_code);
        format_time += start.elapsed();
        changed += usize::from(outcome.actually_formatted);
    }
    eprintln!(
        "{} hexstead scripts ({changed} would change): parsed in {parse_time:?}, formatted in \
         {format_time:?}; gdformat takes 7.2 s",
        scripts.len()
    );
}
