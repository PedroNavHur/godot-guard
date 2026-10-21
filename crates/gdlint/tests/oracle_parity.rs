//! The port's `gdlint` command against gdtoolkit 4.5.0's, on the corpus: the same stdout, stderr and exit
//! status, byte for byte. Skips unless GG_CORPUS names the corpus directory (see docs/rust-port.md).
//!
//! The port lints the oracle's own trees (`tools/oracle/dump_tree.py`), so a difference is the linter's,
//! not the parser's. Run with `--nocapture` to see each case's counts and timings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use gdlint::cli;
use gdscript::Tree;
use gdscript::oracle::{Dump, read_line};

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

/// The oracle's trees for the files the port would lint, keyed by file text, as the binary keys them.
fn oracle_trees(
    corpus: &Corpus,
    cwd: &Path,
    args: &[String],
) -> (HashMap<String, Result<Tree, String>>, usize) {
    let config = cli::load_config(cwd).expect("config loads");
    let files = cli::find_gd_files_from_paths(args, &config.excluded_directories, cwd);
    let files: Vec<&String> = files.iter().filter(|f| cwd.join(f).is_file()).collect();
    let mut trees = HashMap::new();
    if files.is_empty() {
        return (trees, 0);
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
        let (path, outcome) = match read_line(line).expect("dump line loads") {
            Dump::Parsed { path, tree, .. } => (path, Ok(tree)),
            Dump::Rejected { path, error } => (path, Err(error["message"].to_string())),
        };
        let source = gdlint::read_source(&cwd.join(&path))
            .expect("read")
            .expect("UTF-8");
        trees.insert(source, outcome);
    }
    (trees, files.len())
}

fn run_port(cwd: &Path, args: &[String], trees: &HashMap<String, Result<Tree, String>>) -> Output {
    let parse = |source: &str| {
        trees
            .get(source)
            .cloned()
            .expect("a tree for every linted file")
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = cli::run(args, cwd, &parse, &mut out, &mut err).expect("write to memory");
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

/// Run one case through both, assert identical output, and print what it covered.
fn compare(corpus: &Corpus, name: &str, cwd: &Path, args: &[&str]) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (trees, files) = oracle_trees(corpus, cwd, &args);
    let rejected = trees.values().filter(|t| t.is_err()).count();
    assert_eq!(
        rejected, 0,
        "{name}: the oracle rejects {rejected} files; parse-error text may differ, so drop them"
    );
    let started = Instant::now();
    let oracle = run_oracle(corpus, cwd, &args);
    let oracle_time = started.elapsed();
    let started = Instant::now();
    let port = run_port(cwd, &args, &trees);
    let port_time = started.elapsed();
    let problems = oracle
        .stderr
        .lines()
        .filter(|l| l.contains(": Error: "))
        .count();
    println!(
        "{name}: {files} files, {problems} problems, exit {}; oracle {oracle_time:.2?}, port {port_time:.2?} (trees preloaded)",
        oracle.status
    );
    let mut failures = Vec::new();
    if oracle.status != port.status {
        failures.push(format!(
            "exit status: oracle {}, port {}",
            oracle.status, port.status
        ));
    }
    for (stream, expected, actual) in [
        ("stdout", &oracle.stdout, &port.stdout),
        ("stderr", &oracle.stderr, &port.stderr),
    ] {
        if let Some(difference) = first_difference(expected, actual) {
            failures.push(format!("{stream} {difference}"));
        } else if expected != actual {
            failures.push(format!("{stream} differs in line endings"));
        }
    }
    assert!(
        failures.is_empty(),
        "{name} ({cwd:?} {args:?}):\n{}",
        failures.join("\n")
    );
}

/// A scratch directory holding hexstead's `.gdlintrc`, or none, to run a case under either config.
fn config_dir(corpus: &Corpus, name: &str, hexstead_rc: bool) -> PathBuf {
    let dir = scratch(name);
    if hexstead_rc {
        std::fs::copy(
            corpus.root.join("hexstead/.gdlintrc"),
            dir.join(".gdlintrc"),
        )
        .expect("copy .gdlintrc");
    } else {
        assert_eq!(
            gdlint::config::find_config_file(&dir),
            None,
            "a gdlintrc above {dir:?} would hide the defaults"
        );
    }
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
    let defaults = config_dir(&corpus, "defaults", false);
    compare(
        &corpus,
        "hexstead-defaults",
        &defaults,
        &[path_arg(&corpus.root.join("hexstead"))],
    );
    let hexstead_rc = config_dir(&corpus, "hexstead-rc", true);
    compare(
        &corpus,
        "wingmen-hexstead-rc",
        &hexstead_rc,
        &[path_arg(&corpus.root.join("wingmen"))],
    );
}

/// Copy `.gd` files into `dir`, keeping only those the oracle parses.
fn keep_parsed(corpus: &Corpus, dir: &Path) -> usize {
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
            std::fs::remove_file(dir.join(path)).expect("drop rejected file");
            rejected += 1;
        }
    }
    rejected
}

#[test]
fn gdtoolkit_linter_test_snippets() {
    let Some(corpus) = Corpus::find() else { return };
    let cases = scratch("cases");
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
    let rejected = keep_parsed(&corpus, &cases);
    println!(
        "gdtoolkit test snippets: {} extracted, {rejected} not GDScript",
        String::from_utf8_lossy(&extract.stdout).trim()
    );
    compare(&corpus, "snippets-defaults", &cases, &["."]);
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
    let dir = config_dir(&corpus, "valid-scripts", false);
    let source = corpus.root.join("gdtoolkit-src/tests/valid-gd-scripts");
    for entry in std::fs::read_dir(&source).expect("list valid-gd-scripts") {
        let path = entry.expect("entry").path();
        if path.extension().is_some_and(|e| e == "gd") {
            std::fs::copy(&path, dir.join(path.file_name().expect("name"))).expect("copy script");
        }
    }
    let rejected = keep_parsed(&corpus, &dir);
    println!("valid-gd-scripts: {rejected} rejected by the oracle parser");
    compare(&corpus, "valid-scripts", &dir, &["."]);
}

#[test]
fn command_line_edge_cases() {
    let Some(corpus) = Corpus::find() else { return };
    let dir = config_dir(&corpus, "cli", false);
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
    let rc = config_dir(&corpus, "cli-rc", false);
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
        run_port(&port_dir, &dump, &HashMap::new()),
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
        run_port(&port_dir, &dump, &HashMap::new()),
    );
    assert_eq!((oracle.status, port.status), (1, 1));
}
