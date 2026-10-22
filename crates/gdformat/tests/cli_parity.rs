//! The `gdformat` binary against the oracle's: each scenario runs both on identical copies of a
//! project and compares stdout, stderr, exit status, and the files left behind. Skips when GG_CORPUS is
//! unset.
//!
//! Allowed deviations: the text of a parse error, where stderr is compared up to it, and the warning
//! docopt prints before the usage for an unknown option.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Run {
    stdout: String,
    stderr: String,
    status: i32,
    files: BTreeMap<PathBuf, Vec<u8>>,
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn snapshot(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            snapshot(root, &path, files);
        } else {
            files.insert(
                path.strip_prefix(root).unwrap().to_owned(),
                std::fs::read(&path).unwrap(),
            );
        }
    }
}

fn run(
    program: &Path,
    project: &Path,
    workdir: &Path,
    args: &[&str],
    stdin: &[u8],
    corpus: &Path,
) -> Run {
    let _ = std::fs::remove_dir_all(workdir);
    copy_tree(project, workdir);
    let mut child = Command::new(program)
        .args(args)
        .current_dir(workdir)
        .env("GG_CORPUS", corpus)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let output = child.wait_with_output().unwrap();
    let mut files = BTreeMap::new();
    snapshot(workdir, workdir, &mut files);
    Run {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        status: output.status.code().unwrap_or(-1),
        files,
    }
}

/// stderr up to the first parse error's message: its `path:` header and blank line stay.
fn up_to_parse_error(stderr: &str) -> &str {
    match stderr.find(":\n\n") {
        Some(at) => &stderr[..at + 3],
        None => stderr,
    }
}

struct Scenario {
    name: &'static str,
    project: PathBuf,
    args: &'static [&'static str],
    stdin: &'static [u8],
    parse_error: bool,
}

fn fixture_project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cli")
}

#[test]
fn cli_matches_gdformat() {
    let Some(corpus) = std::env::var_os("GG_CORPUS").map(PathBuf::from) else {
        eprintln!("GG_CORPUS unset; skipping the gdformat CLI parity run");
        return;
    };
    let fixtures = fixture_project();
    let scenarios = [
        Scenario {
            name: "wingmen-check",
            project: corpus.join("wingmen"),
            args: &["--check", "."],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "wingmen-format",
            project: corpus.join("wingmen"),
            args: &["core", "ui", "tests"],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "safety-check",
            project: fixtures.join("safe"),
            args: &["-c", "."],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "safety-format",
            project: fixtures.join("safe"),
            args: &["."],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "fast",
            project: fixtures.join("safe"),
            args: &["--fast", "--check", "."],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "options",
            project: fixtures.join("plain"),
            args: &["-l", "40", "--use-spaces=2", "-c", "."],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "missing",
            project: fixtures.join("plain"),
            args: &["nonexistent.gd", "."],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "config-yaml-1.1",
            project: fixtures.join("yaml11"),
            args: &["--check", "."],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "empty-path",
            project: fixtures.join("plain"),
            args: &["--check", ""],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "rejected",
            project: fixtures.join("broken"),
            args: &["--check", "."],
            stdin: b"",
            parse_error: true,
        },
        Scenario {
            name: "stdin",
            project: fixtures.join("plain"),
            args: &["-"],
            stdin: b"func f(a,b):\r\n\treturn a+b\r\n",
            parse_error: false,
        },
        Scenario {
            name: "usage",
            project: fixtures.join("plain"),
            args: &[],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "bogus",
            project: fixtures.join("plain"),
            args: &["--bogus", "a.gd"],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "version",
            project: fixtures.join("plain"),
            args: &["--version"],
            stdin: b"",
            parse_error: false,
        },
        Scenario {
            name: "dump-config",
            project: fixtures.join("plain"),
            args: &["--dump-default-config"],
            stdin: b"",
            parse_error: false,
        },
    ];
    let oracle = corpus.join("oracle-venv/bin/gdformat");
    let port = PathBuf::from(env!("CARGO_BIN_EXE_gdformat"));
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gdformat-cli");
    let mut failures = Vec::new();
    for scenario in &scenarios {
        let expected = run(
            &oracle,
            &scenario.project,
            &scratch.join("oracle"),
            scenario.args,
            scenario.stdin,
            &corpus,
        );
        let actual = run(
            &port,
            &scenario.project,
            &scratch.join("port"),
            scenario.args,
            scenario.stdin,
            &corpus,
        );
        let stderr_matches = match scenario.parse_error {
            true => up_to_parse_error(&expected.stderr) == up_to_parse_error(&actual.stderr),
            // docopt's warning names its internal Option object; the usage after it must match.
            false if scenario.name == "bogus" => expected
                .stderr
                .ends_with(&actual.stderr[actual.stderr.find("Usage:").unwrap()..]),
            false => expected.stderr == actual.stderr,
        };
        let checks = [
            ("stdout", expected.stdout == actual.stdout),
            ("stderr", stderr_matches),
            ("exit status", expected.status == actual.status),
            ("files", expected.files == actual.files),
        ];
        for (what, same) in checks {
            if !same {
                failures.push(format!("{}: {what} differs", scenario.name));
            }
        }
        if !checks.iter().all(|(_, same)| *same) {
            eprintln!(
                "== {}\noracle {}:\n{}{}\nport {}:\n{}{}",
                scenario.name,
                expected.status,
                expected.stdout,
                expected.stderr,
                actual.status,
                actual.stdout,
                actual.stderr
            );
        }
        eprintln!("{:>14}: exit {}", scenario.name, actual.status);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
