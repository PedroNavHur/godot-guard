//! The port's `gdlint` command against gdtoolkit 4.5.0's, on the corpus: the same stdout, stderr and exit
//! status, byte for byte. Skips unless GG_CORPUS names the corpus directory (see docs/rust-port.md).
//!
//! Each case runs twice: on the oracle's own trees (`tools/oracle/dump_tree.py`), where a difference is the
//! linter's, and on `gdscript::parse`'s, the port as it ships. Parse errors compare without lark's expected
//! terminals and previous tokens (see `src/parse_error.rs`). Run with `--nocapture` for counts and timings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use gdlint::cli;
use gdscript::oracle::{Dump, read_line};
use gdscript::{ParseError, Tree};

struct Corpus {
    root: PathBuf,
}

impl Corpus {
    fn find() -> Option<Corpus> {
        let root = PathBuf::from(std::env::var_os("GG_CORPUS")?);
        assert!(
            root.is_dir(),
            "GG_CORPUS={} is not a directory",
            root.display()
        );
        Some(Corpus { root })
    }

    fn python(&self) -> PathBuf {
        self.root.join("oracle-venv/bin/python")
    }

    fn gdlint(&self) -> PathBuf {
        self.root.join("oracle-venv/bin/gdlint")
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh empty directory for one case, by its real path: Python's `os.getcwd()` resolves symlinks (macOS's
/// `/var` is `/private/var`), and the port's binary reads the working directory the same way.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gdlint-parity-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("clear scratch dir");
    }
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir.canonicalize().expect("canonical scratch dir")
}

struct Output {
    stdout: String,
    stderr: String,
    status: i32,
}

/// The oracle's trees for the files the port would lint, keyed by file text; the files it rejects; and how
/// many files there are.
fn oracle_trees(
    corpus: &Corpus,
    cwd: &Path,
    args: &[String],
) -> (HashMap<String, Tree>, usize, usize) {
    let config = cli::load_config(cwd).expect("config loads");
    let files = cli::find_gd_files_from_paths(args, &config.excluded_directories, cwd);
    let files: Vec<&String> = files.iter().filter(|f| cwd.join(f).is_file()).collect();
    let (mut trees, mut rejected) = (HashMap::new(), 0);
    if files.is_empty() {
        return (trees, 0, 0);
    }
    let dump = Command::new(corpus.python())
        .arg(repo_root().join("tools/oracle/dump_tree.py"))
        .args(&files)
        .current_dir(cwd)
        .output()
        .expect("run dump_tree.py");
    assert!(
        dump.status.success(),
        "dump_tree.py failed: {}",
        String::from_utf8_lossy(&dump.stderr)
    );
    for line in String::from_utf8(dump.stdout)
        .expect("dump is UTF-8")
        .lines()
    {
        let Dump::Parsed { path, tree, .. } = read_line(line).expect("dump line loads") else {
            rejected += 1;
            continue;
        };
        let source = gdlint::read_source(&cwd.join(&path))
            .expect("read")
            .expect("UTF-8");
        trees.insert(source, tree);
    }
    (trees, rejected, files.len())
}

fn run_port(cwd: &Path, args: &[String], parse: &cli::Parse) -> Output {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = cli::run(args, cwd, parse, &mut out, &mut err).expect("write to memory");
    Output {
        stdout: String::from_utf8(out).expect("UTF-8 stdout"),
        stderr: String::from_utf8(err).expect("UTF-8 stderr"),
        status,
    }
}

fn run_oracle(corpus: &Corpus, cwd: &Path, args: &[String]) -> Output {
    let output = Command::new(corpus.gdlint())
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run oracle gdlint");
    Output {
        stdout: String::from_utf8(output.stdout).expect("UTF-8 stdout"),
        stderr: String::from_utf8(output.stderr).expect("UTF-8 stderr"),
        status: output.status.code().expect("oracle exited"),
    }
}

/// The first line where two texts differ, with a little context, or None.
fn first_difference(expected: &str, actual: &str) -> Option<String> {
    let (e, a): (Vec<_>, Vec<_>) = (expected.lines().collect(), actual.lines().collect());
    let index = (0..e.len().max(a.len())).find(|&i| e.get(i) != a.get(i))?;
    let show = |lines: &[&str]| {
        lines
            .iter()
            .skip(index.saturating_sub(2))
            .take(5)
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    };
    Some(format!(
        "line {}: oracle has {} lines, port {}\n--- oracle\n{}\n--- port\n{}",
        index + 1,
        e.len(),
        a.len(),
        show(&e),
        show(&a)
    ))
}

/// `text` without what the port cannot reproduce of a parse error: the lines listing lark's expected
/// terminals and its previous tokens, and the blank line before a previous-tokens line.
fn without_lark_lists(text: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut in_list = false;
    for line in text.lines() {
        match LarkLine::of(line, in_list) {
            LarkLine::ExpectedHeader => in_list = true,
            LarkLine::PreviousTokens => pop_blank_before_previous_tokens(&mut kept, in_list),
            LarkLine::ListItem => {}
            LarkLine::Other => {
                in_list = false;
                kept.push(line);
            }
        }
    }
    kept.join("\n")
}

/// What a line of a parse error is, to lark's lists.
enum LarkLine {
    ExpectedHeader,
    PreviousTokens,
    ListItem,
    Other,
}

impl LarkLine {
    /// `line`, after a line of the expected-terminals list if `in_list`.
    fn of(line: &str, in_list: bool) -> LarkLine {
        if line.starts_with("Expected one of:") {
            return LarkLine::ExpectedHeader;
        }
        if line.starts_with("Previous tokens:") {
            return LarkLine::PreviousTokens;
        }
        let is_list_line = line.is_empty() || line.starts_with("\t* ");
        if in_list && is_list_line {
            return LarkLine::ListItem;
        }
        LarkLine::Other
    }
}

fn pop_blank_before_previous_tokens(kept: &mut Vec<&str>, in_list: bool) {
    if kept.last() == Some(&"") && !in_list {
        kept.pop();
    }
}

/// Run one case through the oracle and, on both trees, the port; assert identical output; print counts.
fn compare(corpus: &Corpus, name: &str, cwd: &Path, args: &[&str]) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (trees, rejected, files) = oracle_trees(corpus, cwd, &args);
    let started = Instant::now();
    let oracle = run_oracle(corpus, cwd, &args);
    let oracle_time = started.elapsed();
    let problems = oracle
        .stderr
        .lines()
        .filter(|l| l.contains(": Error: "))
        .count();
    let started = Instant::now();
    let parsed = run_port(cwd, &args, &gdscript::parse);
    let parse_time = started.elapsed();
    println!(
        "{name}: {files} files, {rejected} rejected, {problems} problems, exit {}; oracle {oracle_time:.2?}, port {parse_time:.2?}",
        oracle.status
    );
    let mut failures = same_output(&oracle, &parsed, Trees::Parser);
    if rejected == 0 {
        let parse = |source: &str| -> Result<Tree, ParseError> {
            Ok(trees
                .get(source)
                .cloned()
                .expect("a tree for every linted file"))
        };
        let port = run_port(cwd, &args, &parse);
        failures.extend(same_output(&oracle, &port, Trees::Oracle));
    }
    assert!(
        failures.is_empty(),
        "{name} ({cwd:?} {args:?}):\n{}",
        failures.join("\n")
    );
}

/// Where the port's trees came from.
#[derive(Clone, Copy)]
enum Trees {
    Parser,
    Oracle,
}

impl std::fmt::Display for Trees {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Trees::Parser => f.write_str("parser trees"),
            Trees::Oracle => f.write_str("oracle trees"),
        }
    }
}

/// The stderr of both, as compared when the port parsed the files itself: lark's lists drop out of both, and
/// where Python died with a traceback only its last line must match (a mismatch goes to `failures`).
fn normalise_parser_stderr(
    oracle: &Output,
    port: &Output,
    failures: &mut Vec<String>,
) -> (String, String) {
    let (mut oracle_err, mut port_err) = (oracle.stderr.clone(), port.stderr.clone());
    if let Some(at) = oracle_err.find("Traceback (most recent call last):") {
        let exception = oracle_err.lines().last().unwrap_or_default().to_owned();
        if !port_err.trim_end().ends_with(&format!(": {exception}")) {
            failures.push(format!(
                "{}: Python died with {exception}; the port printed {port_err}",
                Trees::Parser
            ));
        }
        oracle_err.truncate(at);
        port_err.truncate(port_err.trim_end().rfind('\n').map_or(0, |n| n + 1));
    }
    (
        without_lark_lists(&oracle_err),
        without_lark_lists(&port_err),
    )
}

/// How the port's output differs from the oracle's.
fn same_output(oracle: &Output, port: &Output, trees: Trees) -> Vec<String> {
    let mut failures = Vec::new();
    if oracle.status != port.status {
        failures.push(format!(
            "{trees}: exit status: oracle {}, port {}",
            oracle.status, port.status
        ));
    }
    let (oracle_err, port_err) = match trees {
        Trees::Parser => normalise_parser_stderr(oracle, port, &mut failures),
        Trees::Oracle => (oracle.stderr.clone(), port.stderr.clone()),
    };
    for (stream, expected, actual) in [
        ("stdout", &oracle.stdout, &port.stdout),
        ("stderr", &oracle_err, &port_err),
    ] {
        if let Some(difference) = first_difference(expected, actual) {
            failures.push(format!("{trees}: {stream} {difference}"));
        } else if expected != actual {
            failures.push(format!("{trees}: {stream} differs in line endings"));
        }
    }
    failures
}

/// A scratch directory with no `.gdlintrc` in it or above, to run a case under the defaults.
fn defaults_dir(name: &str) -> PathBuf {
    let dir = scratch(name);
    assert_eq!(
        gdlint::config::find_config_file(&dir),
        None,
        "a gdlintrc above {dir:?} would hide the defaults"
    );
    dir
}

/// A scratch directory holding hexstead's `.gdlintrc`, to run a case under it.
fn hexstead_rc_dir(corpus: &Corpus, name: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::copy(
        corpus.root.join("hexstead/.gdlintrc"),
        dir.join(".gdlintrc"),
    )
    .expect("copy .gdlintrc");
    dir
}

fn path_arg(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

#[test]
fn hexstead_with_its_gdlintrc() {
    let Some(corpus) = Corpus::find() else { return };
    compare(&corpus, "hexstead", &corpus.root.join("hexstead"), &["."]);
}

#[test]
fn wingmen_with_defaults() {
    let Some(corpus) = Corpus::find() else { return };
    let wingmen = corpus.root.join("wingmen");
    assert_eq!(
        gdlint::config::find_config_file(&wingmen),
        None,
        "wingmen must lint under the defaults"
    );
    compare(&corpus, "wingmen", &wingmen, &["."]);
}

#[test]
fn corpora_under_the_other_config() {
    let Some(corpus) = Corpus::find() else { return };
    let defaults = defaults_dir("defaults");
    compare(
        &corpus,
        "hexstead-defaults",
        &defaults,
        &[path_arg(&corpus.root.join("hexstead"))],
    );
    let hexstead_rc = hexstead_rc_dir(&corpus, "hexstead-rc");
    compare(
        &corpus,
        "wingmen-hexstead-rc",
        &hexstead_rc,
        &[path_arg(&corpus.root.join("wingmen"))],
    );
}

/// Move the `.gd` files in `dir` the oracle rejects into `rejected`, so `dir` runs on both trees; how many.
fn set_rejected_aside(corpus: &Corpus, dir: &Path, rejected_dir: &Path) -> usize {
    let names: Vec<String> = std::fs::read_dir(dir)
        .expect("list")
        .map(|e| {
            e.expect("entry")
                .file_name()
                .into_string()
                .expect("UTF-8 name")
        })
        .filter(|n| n.ends_with(".gd"))
        .collect();
    let dump = Command::new(corpus.python())
        .arg(repo_root().join("tools/oracle/dump_tree.py"))
        .args(&names)
        .current_dir(dir)
        .output()
        .expect("run dump_tree.py");
    let mut rejected = 0;
    for line in String::from_utf8(dump.stdout).expect("UTF-8").lines() {
        if let Dump::Rejected { path, .. } = read_line(line).expect("dump line") {
            std::fs::rename(dir.join(&path), rejected_dir.join(&path)).expect("move rejected file");
            rejected += 1;
        }
    }
    rejected
}

#[test]
fn gdtoolkit_linter_test_snippets() {
    let Some(corpus) = Corpus::find() else { return };
    let (cases, rejected_cases) = (scratch("cases"), scratch("cases-rejected"));
    let extract = Command::new(corpus.python())
        .arg("-I")
        .arg(repo_root().join("tools/oracle/lint_cases.py"))
        .arg(corpus.root.join("gdtoolkit-src/tests/linter"))
        .arg(&cases)
        .output()
        .expect("run lint_cases.py");
    assert!(
        extract.status.success(),
        "lint_cases.py: {}",
        String::from_utf8_lossy(&extract.stderr)
    );
    let rejected = set_rejected_aside(&corpus, &cases, &rejected_cases);
    println!(
        "gdtoolkit test snippets: {} extracted, {rejected} not GDScript",
        String::from_utf8_lossy(&extract.stdout).trim()
    );
    compare(&corpus, "snippets-defaults", &cases, &["."]);
    compare(&corpus, "snippets-rejected", &rejected_cases, &["."]);
    std::fs::copy(
        corpus.root.join("hexstead/.gdlintrc"),
        cases.join(".gdlintrc"),
    )
    .expect("copy .gdlintrc");
    compare(&corpus, "snippets-hexstead-rc", &cases, &["."]);
}

#[test]
fn gdtoolkit_valid_scripts() {
    let Some(corpus) = Corpus::find() else { return };
    let dir = defaults_dir("valid-scripts");
    let source = corpus.root.join("gdtoolkit-src/tests/valid-gd-scripts");
    for entry in std::fs::read_dir(&source).expect("list valid-gd-scripts") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "gd") {
            std::fs::copy(&path, dir.join(path.file_name().expect("name"))).expect("copy script");
        }
    }
    let rejected_dir = scratch("valid-scripts-rejected");
    let rejected = set_rejected_aside(&corpus, &dir, &rejected_dir);
    println!("valid-gd-scripts: {rejected} rejected by the oracle parser");
    compare(&corpus, "valid-scripts", &dir, &["."]);
    compare(&corpus, "valid-scripts-rejected", &rejected_dir, &["."]);
}

#[test]
fn command_line_edge_cases() {
    let Some(corpus) = Corpus::find() else { return };
    let dir = defaults_dir("cli");
    std::fs::write(dir.join("ok.gd"), "pass\n").expect("write");
    std::fs::write(dir.join("bad.gd"), "var Xx = 1\r\nfunc f(a):\r\n\tpass \n").expect("write");
    std::fs::create_dir(dir.join("sub.gd")).expect("mkdir");
    std::fs::write(dir.join("sub.gd/inner.gd"), "var Yy\n").expect("write");
    let cases: &[&[&str]] = &[
        &[],
        &["-h"],
        &["ok.gd", "--version"],
        &["--bogus", "ok.gd"],
        &["--bogus=it's", "ok.gd"],
        &["-d", "-v"],
        &["ok.gd", "-d"],
        &["-vv", "ok.gd"],
        &["--ver", "ok.gd"],
        &["--verb", "ok.gd"],
        &["--verbose=1", "ok.gd"],
        &["-xd"],
        &["--", "-x.gd"],
        &["-", "-1", "nonexistent.gd"],
        &["ok.gd", "bad.gd", "sub.gd", "."],
        &["-v", "ok.gd"],
    ];
    for args in cases {
        compare(&corpus, &format!("cli {args:?}"), &dir, args);
    }
    // A config with every kind of entry gdlint logs, in a directory of its own.
    let rc = defaults_dir("cli-rc");
    std::fs::write(rc.join("ok.gd"), "pass\n").expect("write");
    std::fs::write(
        rc.join("gdlintrc"),
        "disable: [unused-argument]\nmax-returns: 0x10\n1: on\nexcluded_directories: !!set {.x}\n",
    )
    .expect("write");
    compare(&corpus, "cli verbose with gdlintrc", &rc, &["-v", "ok.gd"]);
    let (oracle_dir, port_dir) = (scratch("dump-oracle"), scratch("dump-port"));
    let dump = ["-d".to_string()];
    let (oracle, port) = (
        run_oracle(&corpus, &oracle_dir, &dump),
        run_port(&port_dir, &dump, &gdscript::parse),
    );
    assert_eq!(
        (oracle.status, &oracle.stdout, &oracle.stderr),
        (port.status, &port.stdout, &port.stderr)
    );
    let read = |dir: &Path| std::fs::read_to_string(dir.join("gdlintrc")).expect("dumped gdlintrc");
    assert_eq!(
        read(&oracle_dir),
        read(&port_dir),
        "gdlint -d writes a different gdlintrc"
    );
    // A second -d finds the file: the oracle dies on an assertion, the port says so; both exit 1.
    let (oracle, port) = (
        run_oracle(&corpus, &oracle_dir, &dump),
        run_port(&port_dir, &dump, &gdscript::parse),
    );
    assert_eq!((oracle.status, port.status), (1, 1));
}

#[test]
fn parse_errors() {
    let Some(corpus) = Corpus::find() else { return };
    let dir = defaults_dir("parse-errors");
    let long = format!("var s = \"{}\" )\n", "a".repeat(50));
    let scripts = [
        ("token.gd", "func f():\n\tvar x = )\n"),
        ("newline.gd", "func f():\n\tvar x = $\n"),
        ("dedent.gd", "func f():\n\t\tpass\n\tpass\n"),
        ("quotes.gd", "var a = \"it's\" \"x\"\nvar s = 'it''s'\n"),
        ("unicode.gd", "var s = \"\u{e9}\u{3000}\" )\n"),
        ("eof.gd", "func f():\n\tif (\n"),
        ("indent.gd", "var a = 1\n\t\tvar b = 2\n"),
        ("first.gd", "x"),
        ("character.gd", "var x = 1 ? 2\n"),
        ("tabs.gd", "func f():\n\tif true:\n\t\tvar\tx = = 1\n"),
        ("long.gd", long.as_str()),
        ("crlf.gd", "func f():\r\n\tvar x = )\r\n"),
        ("fine.gd", "var Bad_name = 1\n"),
    ];
    for (name, text) in scripts {
        std::fs::write(dir.join(name), text).expect("write script");
    }
    compare(&corpus, "parse errors", &dir, &["."]);
}
