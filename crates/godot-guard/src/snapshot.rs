//! The selected staged scripts as they will be committed, exported from the git index for the pre-commit gate.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::{BASELINE_NAME, Config, hidden};
use crate::error::{Failure, Result};

/// Each tool takes the first of its names found from the working directory upward.
const TOOL_CONFIGS: [[&str; 2]; 2] = [["gdlintrc", ".gdlintrc"], ["gdformatrc", ".gdformatrc"]];
const SYMLINK_MODE: &str = "120000";

/// git's stdout; a failed git is a crash, as 0.4.0's `check=True` is.
fn git(config: &Config, args: &[&str], stdin: Option<&str>) -> Result<String> {
    let mut child = Command::new("git")
        .args(args)
        .current_dir(&config.root)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Failure::crash("git", e))?;
    if let Some(input) = stdin {
        let mut pipe = child.stdin.take().expect("a piped stdin");
        pipe.write_all(input.as_bytes())
            .map_err(|e| Failure::crash("git", e))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| Failure::crash("git", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Failure::Crash(format!(
            "git {} failed: {}",
            args.join(" "),
            stderr.trim_end()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Scanned scripts added, copied, modified, renamed, or retyped in the index, relative to the root.
///
/// Returns the regular files and, apart, the symlinks, which have no content of their own to check.
pub fn staged(config: &Config) -> Result<(Vec<String>, Vec<String>)> {
    let names = git(
        config,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=ACMRT",
            "--relative",
            "-z",
        ],
        None,
    )?;
    let scanned: Vec<&str> = names
        .split('\0')
        .filter(|name| config.is_scanned(name))
        .collect();
    if scanned.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut args = vec!["--literal-pathspecs", "ls-files", "-s", "-z", "--"];
    args.extend(&scanned);
    let output = git(config, &args, None)?;
    let entries: BTreeSet<(&str, &str)> = output
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.split_once('\t').unwrap_or((entry, "")))
        .collect();
    let links: BTreeSet<&str> = entries
        .iter()
        .filter(|(info, _)| info.starts_with(SYMLINK_MODE))
        .map(|(_, path)| *path)
        .collect();
    let paths: BTreeSet<&str> = entries
        .iter()
        .map(|(_, path)| *path)
        .filter(|path| !links.contains(path))
        .collect();
    let owned = |set: BTreeSet<&str>| set.into_iter().map(str::to_owned).collect();
    Ok((owned(paths), owned(links)))
}

/// Scripts in the index below the root, submodules' included, other than `chosen` and hidden ones.
pub fn indexed(config: &Config, chosen: &[String]) -> Result<Vec<String>> {
    let names = git(config, &["ls-files", "-z", "--recurse-submodules"], None)?;
    Ok(names
        .split('\0')
        .filter(|name| name.ends_with(".gd") && !hidden(name) && !chosen.iter().any(|c| c == name))
        .map(str::to_owned)
        .collect())
}

fn nearest(start: &Path, names: &[&str]) -> Option<PathBuf> {
    start
        .ancestors()
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .find(|p| p.is_file())
}

/// Copy the gdlint and gdformat configs the tools would find from `source` into `target`.
fn copy_tool_configs(source: &Path, target: &Path) -> Result<()> {
    for names in TOOL_CONFIGS {
        if let Some(found) = nearest(source, &names) {
            let to = target.join(found.file_name().expect("a config file"));
            fs::copy(&found, &to).map_err(|e| Failure::crash(found.display(), e))?;
        }
    }
    Ok(())
}

/// A config rooted in a temporary copy of the index's `chosen` scripts and baseline, plus the tool configs;
/// the copy lives as long as the returned directory.
///
/// The baseline falls back to the working tree's when the index has none.
pub fn snapshot(config: &Config, chosen: &[String]) -> Result<(tempfile::TempDir, Config)> {
    let top = git(config, &["rev-parse", "--show-toplevel"], None)?;
    let top = fs::canonicalize(top.trim()).map_err(|e| Failure::crash(top.trim(), e))?;
    let temp = tempfile::tempdir().map_err(|e| Failure::crash("a temporary directory", e))?;
    let base =
        fs::canonicalize(temp.path()).map_err(|e| Failure::crash(temp.path().display(), e))?;
    let below = config
        .root
        .strip_prefix(&top)
        .map_err(|e| Failure::crash(config.root.display(), e))?;
    let root = base.join(below);
    let tracked = git(config, &["ls-files", "-z", "--", BASELINE_NAME], None)?;
    let tracked: Vec<&str> = tracked
        .split('\0')
        .filter(|name| !name.is_empty())
        .collect();
    let listed: Vec<&str> = chosen
        .iter()
        .map(String::as_str)
        .chain(tracked.iter().copied())
        .collect();
    let prefix = format!("--prefix={}/", base.display());
    git(
        config,
        &["checkout-index", "-z", "--stdin", &prefix],
        Some(&listed.join("\0")),
    )?;
    fs::create_dir_all(&root).map_err(|e| Failure::crash(root.display(), e))?;
    copy_tool_configs(&config.root, &root)?;
    let baseline = config.baseline_path();
    if !tracked.contains(&BASELINE_NAME) && baseline.is_file() {
        fs::copy(&baseline, root.join(BASELINE_NAME))
            .map_err(|e| Failure::crash(baseline.display(), e))?;
    }
    let mut sources = config.clone();
    sources.root = root;
    Ok((temp, sources))
}
