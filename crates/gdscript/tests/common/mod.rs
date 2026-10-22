//! Comparing the crate's output with an oracle dump, shared by the parity tests.

use std::path::Path;

use gdscript::oracle::Dump;
use gdscript::{Node, Tree};

/// The text Python's dumper parses: decoded with errors="replace", newlines translated as text mode does.
fn read_like_python(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let text = String::from_utf8_lossy(&bytes);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Ok(true) for a matching parse, Ok(false) for a matching rejection, Err with a description otherwise.
/// Relative dump paths resolve against `base`.
pub fn compare(dump: &Dump, base: &Path) -> Result<bool, String> {
    match dump {
        Dump::Parsed {
            path,
            tree,
            comments,
        } => {
            let source = read_like_python(&base.join(path));
            let ours = gdscript::parse(&source)
                .map_err(|e| format!("{path}: rejected ({e}); gdtoolkit parses it"))?;
            first_difference(&ours, tree, "tree")
                .map_or(Ok(()), |d| Err(format!("{path}: {d}")))?;
            let ours = gdscript::parse_comments(&source)
                .map_err(|e| format!("{path}: comments rejected ({e}); gdtoolkit parses them"))?;
            first_difference(&ours, comments, "comments")
                .map_or(Ok(true), |d| Err(format!("{path}: {d}")))
        }
        Dump::Rejected { path, error } => {
            let source = read_like_python(&base.join(path));
            let expected_line = error["line"].as_u64().map(|n| n as usize);
            let ours = gdscript::parse(&source).and_then(|_| gdscript::parse_comments(&source));
            match ours {
                Err(e) if e.line == expected_line => Ok(false),
                Err(e) => Err(format!(
                    "{path}: rejected at line {:?} ({e}); gdtoolkit at {expected_line:?}: {error}",
                    e.line
                )),
                Ok(_) => Err(format!("{path}: parsed; gdtoolkit rejects it: {error}")),
            }
        }
    }
}

/// Where two trees first differ, as a path of child indices, or None when they are equal.
fn first_difference(ours: &Tree, theirs: &Tree, at: &str) -> Option<String> {
    if ours.data != theirs.data
        || ours.meta != theirs.meta
        || ours.children.len() != theirs.children.len()
    {
        return Some(format!(
            "{at}: ours {} {:?} with {} children, gdtoolkit's {} {:?} with {}",
            ours.data,
            ours.meta,
            ours.children.len(),
            theirs.data,
            theirs.meta,
            theirs.children.len()
        ));
    }
    ours.children
        .iter()
        .zip(&theirs.children)
        .enumerate()
        .find_map(|(index, (a, b))| child_difference(a, b, format!("{at}/{index}")))
}

/// Where two children differ, below `here`, or None when they are equal.
fn child_difference(ours: &Node, theirs: &Node, here: String) -> Option<String> {
    match (ours, theirs) {
        (Node::Tree(a), Node::Tree(b)) => first_difference(a, b, &format!("{here}:{}", a.data)),
        (a, b) if a != b => Some(format!("{here}: ours {a:?}, gdtoolkit's {b:?}")),
        _ => None,
    }
}
