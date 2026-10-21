//! Time single-threaded parsing of every `.gd` file under a directory: `parse`, then `parse_comments`.
//!
//! cargo run --release -p gdscript --example parse_corpus -- DIR

use std::path::{Path, PathBuf};
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: parse_corpus DIR");
    let mut files = Vec::new();
    collect_gd(Path::new(&dir), &mut files);
    files.sort();
    let sources: Vec<String> = files
        .iter()
        .map(|path| std::fs::read_to_string(path).expect("read script"))
        .collect();
    let bytes: usize = sources.iter().map(String::len).sum();

    let started = Instant::now();
    let parsed = sources
        .iter()
        .filter(|s| gdscript::parse(s).is_ok())
        .count();
    let parse_time = started.elapsed();
    let started = Instant::now();
    let comments = sources
        .iter()
        .filter(|s| gdscript::parse_comments(s).is_ok())
        .count();
    let comments_time = started.elapsed();

    println!(
        "{} files, {bytes} bytes; {parsed} parsed in {parse_time:.3?}, {comments} comment trees in {comments_time:.3?}",
        files.len()
    );
}

fn collect_gd(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_gd(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "gd") {
            files.push(path);
        }
    }
}
