//! The oracle's gdformat cases: generated once per corpus state by `tools/oracle/gdformat_cases.py`,
//! cached under `$CARGO_TARGET_TMPDIR`, and streamed back one case at a time. Each test binary uses part
//! of this module.

#![allow(dead_code)]

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

use gdformat::{ParsedCode, SafetyError};
use gdscript::Tree;
use gdscript::oracle::node;
use serde_json::Value;

pub enum Expected {
    Formatted(String),
    /// gdtoolkit parsed the code, then raised this exception while formatting it.
    Crashed(String),
}

pub struct Case {
    pub id: String,
    pub variant: String,
    pub code: String,
    pub line_length: i64,
    pub spaces: Option<usize>,
    /// None when gdtoolkit rejected the code: a parser case, not a formatter one.
    pub trees: Option<(Tree, Tree)>,
    pub expected: Expected,
    /// check_formatting_safety's verdict: "ok" or the violation's name; None when not run.
    pub safety: Option<String>,
    /// The trees of the expected output, when the oracle ran with --formatted-trees.
    pub formatted_trees: Option<(Tree, Tree)>,
}

/// The `.gd` files of the corpus, sorted: hexstead, wingmen, and gdtoolkit's formatter and parser data.
pub fn corpus_files(corpus: &Path) -> Vec<PathBuf> {
    let tests = corpus.join("gdtoolkit-src/tests");
    let roots = [
        corpus.join("hexstead"),
        corpus.join("wingmen"),
        tests.join("formatter/input-output-pairs"),
        tests.join("formatter/big-input-files"),
        tests.join("valid-gd-scripts"),
    ];
    let mut files = Vec::new();
    for root in roots {
        assert!(
            root.is_dir(),
            "corpus directory {} is missing",
            root.display()
        );
        walk(&root, &mut files);
    }
    files.sort();
    files
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "gd") {
            files.push(path);
        }
    }
}

/// Every case for the corpus under GG_CORPUS, or None when it is unset.
pub fn corpus_cases() -> Option<impl Iterator<Item = Case>> {
    let corpus = PathBuf::from(std::env::var_os("GG_CORPUS")?);
    let files = corpus_files(&corpus);
    let script = oracle_script();
    let mut hasher = DefaultHasher::new();
    std::fs::read(&script).unwrap().hash(&mut hasher);
    for file in &files {
        file.hash(&mut hasher);
        std::fs::read(file).unwrap().hash(&mut hasher);
    }
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("gdformat-cases-{:016x}.jsonl", hasher.finish()));
    if !cache.exists() {
        generate(&corpus, &script, &files, &cache);
    }
    Some(read_cases(&cache))
}

/// The cases in a file `gdformat_cases.py` wrote.
pub fn read_cases(path: &Path) -> impl Iterator<Item = Case> + use<> {
    let reader = BufReader::new(std::fs::File::open(path).unwrap());
    reader.lines().map(|line| parse_case(&line.unwrap()))
}

pub fn oracle_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/oracle/gdformat_cases.py")
}

fn generate(corpus: &Path, script: &Path, files: &[PathBuf], cache: &Path) {
    let partial = cache.with_extension("partial");
    let output = std::fs::File::create(&partial).unwrap();
    let status = Command::new(corpus.join("oracle-venv/bin/python"))
        .arg("-I")
        .arg(script)
        .arg("--formatted-trees")
        .args(files)
        .stdout(output)
        .status()
        .expect("run the oracle");
    assert!(status.success(), "gdformat_cases.py failed: {status}");
    std::fs::rename(&partial, cache).unwrap();
}

fn parse_case(line: &str) -> Case {
    let mut reader = serde_json::Deserializer::from_str(line);
    reader.disable_recursion_limit();
    let value: Value = serde::Deserialize::deserialize(&mut reader).expect("case JSON");
    let text = |key: &str| value[key].as_str().map(str::to_owned);
    let trees = |holder: &Value| match holder["tree"].is_null() {
        true => None,
        false => Some((tree(&holder["tree"]), tree(&holder["comments"]))),
    };
    let expected = match text("expected") {
        Some(formatted) => Expected::Formatted(formatted),
        None => Expected::Crashed(text("error").expect("case with neither output nor error")),
    };
    Case {
        id: text("id").unwrap(),
        variant: text("variant").unwrap(),
        code: text("code").unwrap(),
        line_length: value["line_length"].as_i64().unwrap(),
        spaces: value["spaces"].as_u64().map(|n| n as usize),
        trees: trees(&value),
        expected,
        safety: text("safety"),
        formatted_trees: (!value["formatted"].is_null())
            .then(|| trees(&value["formatted"]))
            .flatten(),
    }
}

fn tree(value: &Value) -> Tree {
    match node(value).expect("oracle tree") {
        gdscript::Node::Tree(tree) => tree,
        gdscript::Node::Token(_) => panic!("oracle tree is a token"),
    }
}

/// How the port did on one case.
pub enum Verdict {
    /// Same output, and the same safety verdict when the oracle ran the checks.
    Identical {
        safety_checked: bool,
    },
    BothCrashed,
    /// gdtoolkit rejected the code: nothing for the formatter to do.
    Rejected,
    Differs {
        expected: String,
        actual: String,
    },
}

/// Format the case as the port does on the oracle's trees and compare with the oracle.
pub fn compare(mut case: Case) -> Verdict {
    let Some((tree, comments)) = case.trees.take() else {
        return Verdict::Rejected;
    };
    let formatted_trees = case.formatted_trees.take();
    judge(&case, ParsedCode { tree, comments }, |_| {
        formatted_trees
            .map(|(tree, comments)| ParsedCode { tree, comments })
            .ok_or(())
    })
}

/// The same on the trees `gdscript` parses, before formatting and in the safety checks.
pub fn compare_parsed(case: &Case) -> Verdict {
    let differs = |expected: &str, actual: String| Verdict::Differs {
        expected: expected.to_owned(),
        actual,
    };
    match (gdformat::parse_code(&case.code), &case.trees) {
        (Err(_), None) => Verdict::Rejected,
        (Ok(_), None) => differs("<gdtoolkit rejects the code>", "<parsed>".to_owned()),
        (Err(error), Some(_)) => differs("<parsed>", format!("<rejected: {error}>")),
        (Ok(given), Some(_)) => judge(case, given, |code| {
            gdformat::parse_code(code).map_err(|_| ())
        }),
    }
}

fn judge(
    case: &Case,
    given: ParsedCode,
    reparse: impl FnOnce(&str) -> Result<ParsedCode, ()>,
) -> Verdict {
    let actual = catch_unwind(AssertUnwindSafe(|| {
        gdformat::format_code(
            &case.code,
            case.line_length,
            case.spaces,
            &given.tree,
            &given.comments,
        )
    }));
    let actual = match (&case.expected, actual) {
        (Expected::Crashed(_), Err(_)) => return Verdict::BothCrashed,
        (_, Ok(actual)) => actual,
        (_, Err(panic)) => {
            let message = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()));
            format!("<panicked: {}>", message.unwrap_or_default())
        }
    };
    let expected = match &case.expected {
        Expected::Formatted(text) => text.clone(),
        Expected::Crashed(error) => format!("<gdtoolkit raised {error}>"),
    };
    if expected != actual {
        return Verdict::Differs { expected, actual };
    }
    let Some(safety) = &case.safety else {
        return Verdict::Identical {
            safety_checked: false,
        };
    };
    let verdict = safety_verdict(case, &actual, &given, reparse);
    match verdict == oracle_verdict(safety) {
        true => Verdict::Identical {
            safety_checked: true,
        },
        false => Verdict::Differs {
            expected: format!("safety: {safety}"),
            actual: format!("safety: {verdict}"),
        },
    }
}

/// The port's safety verdict on a case whose output matched: "ok", a violation's name, or
/// "parse error" when the formatted code doesn't parse.
fn safety_verdict(
    case: &Case,
    formatted: &str,
    given: &ParsedCode,
    reparse: impl FnOnce(&str) -> Result<ParsedCode, ()>,
) -> String {
    let verdict = gdformat::check_formatting_safety(
        &case.code,
        formatted,
        case.line_length,
        given,
        case.spaces,
        reparse,
    );
    match verdict {
        Ok(()) => "ok".to_owned(),
        Err(SafetyError::Violation(violation)) => violation.name().to_owned(),
        Err(SafetyError::Parse(())) => "parse error".to_owned(),
    }
}

/// The oracle's verdict in `safety_verdict`'s terms: lark's exceptions are parse errors.
fn oracle_verdict(safety: &str) -> &str {
    match safety {
        "ok"
        | "TreeInvariantViolation"
        | "FormattingStabilityViolation"
        | "CommentPersistenceViolation" => safety,
        _ => "parse error",
    }
}
