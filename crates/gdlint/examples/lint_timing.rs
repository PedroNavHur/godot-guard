//! Where `gdlint`'s time goes: `cargo run --release -p gdlint --example lint_timing -- PATH...`, run where
//! the paths resolve, as `gdlint PATH...` would be. Finds the files and loads the config first, then times,
//! on one thread, reading every file, parsing it with `gdscript::parse`, and linting it and formatting its
//! problems.

use pycompat::builtins::read_text;
use pycompat::utils::find_gd_files_from_paths;
use std::time::{Duration, Instant};

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().expect("cwd");
    let config = gdlint::cli::load_config(&cwd).expect("config");
    let files = find_gd_files_from_paths(&paths, &config.excluded_directories, &cwd);

    let started = Instant::now();
    let sources: Vec<String> = files
        .iter()
        .map(|path| read_text(&cwd.join(path)).expect("a UTF-8 file"))
        .collect();
    let read = started.elapsed();

    let (mut parse, mut lint) = (Duration::ZERO, Duration::ZERO);
    let (mut output, mut problems, mut rejected) = (String::new(), 0, 0);
    for (path, source) in files.iter().zip(&sources) {
        let started = Instant::now();
        let tree = gdscript::parse(source);
        parse += started.elapsed();
        let Ok(tree) = tree else {
            rejected += 1;
            continue;
        };
        let started = Instant::now();
        for problem in gdlint::lint_code(source, &tree, &config) {
            output.push_str(&format!(
                "{}\n",
                gdlint::Printed {
                    problem: &problem,
                    file_path: path
                }
            ));
            problems += 1;
        }
        lint += started.elapsed();
    }
    println!(
        "{} files ({} KB), {rejected} rejected, {problems} problems; read {read:.2?}, parse {parse:.2?}, lint+format {lint:.2?}",
        files.len(),
        sources.iter().map(String::len).sum::<usize>() / 1000
    );
}
