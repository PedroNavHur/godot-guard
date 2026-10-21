//! Time the port's lint, apart from parsing: `cargo run --release -p gdlint --features oracle --example
//! lint_timing -- DUMP`, run where DUMP's paths resolve (DUMP from `tools/oracle/dump_tree.py`). Loads the
//! config and the dumped trees first, then times, on one thread, reading every file and then linting it and
//! formatting its problems.

use std::time::Instant;

use gdscript::oracle::{Dump, read_line};

fn main() {
    let dump = std::env::args().nth(1).expect("usage: lint_timing DUMP");
    let started = Instant::now();
    let text = std::fs::read_to_string(&dump).expect("read dump");
    let trees: Vec<_> = text
        .lines()
        .filter_map(|line| match read_line(line).expect("dump line") {
            Dump::Parsed { path, tree, .. } => Some((path, tree)),
            Dump::Rejected { .. } => None,
        })
        .collect();
    let loaded = started.elapsed();
    let cwd = std::env::current_dir().expect("cwd");
    let config = gdlint::cli::load_config(&cwd).expect("config");
    let started = Instant::now();
    let sources: Vec<String> = trees
        .iter()
        .map(|(path, _)| {
            gdlint::read_source(path.as_ref())
                .expect("read")
                .expect("UTF-8")
        })
        .collect();
    let read = started.elapsed();
    let started = Instant::now();
    let mut output = String::new();
    let mut problems = 0;
    for ((path, tree), source) in trees.iter().zip(&sources) {
        for problem in gdlint::lint_code(source, tree, &config) {
            output.push_str(&format!(
                "{}\n",
                gdlint::Printed {
                    problem: &problem,
                    file_path: path
                }
            ));
            problems += 1;
        }
    }
    let linted = started.elapsed();
    println!(
        "{} files ({} KB), {problems} problems; trees loaded in {loaded:.2?}; read {read:.2?}; lint+format {linted:.2?}",
        trees.len(),
        sources.iter().map(String::len).sum::<usize>() / 1000
    );
}
