//! `gdtoolkit/common/utils.py`: finding `.gd` files from the paths on a command line.

use std::path::Path;

use crate::os::join;

/// `find_gd_files_from_paths`: each path that is a directory, walked as `os.walk` walks it, top-down, in
/// directory-listing order, not into symlinked or `excluded_directories`; any other path as given.
pub fn find_gd_files_from_paths(
    paths: &[String],
    excluded_directories: &[String],
    cwd: &Path,
) -> Vec<String> {
    let mut files = Vec::new();
    for path in paths {
        if !path.is_empty() && cwd.join(path).is_dir() {
            walk(path, excluded_directories, cwd, &mut files);
        } else {
            files.push(path.clone());
        }
    }
    files
}

fn walk(top: &str, excluded_directories: &[String], cwd: &Path, files: &mut Vec<String>) {
    let Some((dirnames, gd_files)) = scandir(top, cwd) else {
        return;
    };
    files.extend(gd_files);
    for dirname in dirnames
        .iter()
        .filter(|d| !excluded_directories.contains(d))
    {
        let new_path = join(top, dirname);
        if !is_symlink(&cwd.join(&new_path)) {
            walk(&new_path, excluded_directories, cwd, files);
        }
    }
}

/// The subdirectory names of `top` and its `.gd` files as paths from `top`; None when it cannot be listed,
/// as `os.walk` without `onerror` skips it.
fn scandir(top: &str, cwd: &Path) -> Option<(Vec<String>, Vec<String>)> {
    let entries = std::fs::read_dir(cwd.join(top)).ok()?;
    let mut dirnames = Vec::new();
    let mut gd_files = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `Path::is_dir` follows symlinks, as Python's `DirEntry.is_dir()` does.
        if entry.path().is_dir() {
            dirnames.push(name);
        } else if name.ends_with(".gd") {
            gd_files.push(join(top, &name));
        }
    }
    Some((dirnames, gd_files))
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_empty_path_is_a_file_not_the_working_directory() {
        let cwd = std::env::current_dir().unwrap();
        let found = find_gd_files_from_paths(&[String::new()], &[], &cwd);
        assert_eq!(found, [""]);
    }

    #[test]
    fn walks_directories_but_not_excluded_ones() {
        let root = std::env::temp_dir().join(format!("pycompat-walk-{}", std::process::id()));
        std::fs::create_dir_all(root.join("scripts/addons")).unwrap();
        std::fs::write(root.join("scripts/a.gd"), "").unwrap();
        std::fs::write(root.join("scripts/addons/b.gd"), "").unwrap();
        std::fs::write(root.join("scripts/notes.txt"), "").unwrap();
        let paths = ["scripts".to_owned(), "loose.txt".to_owned()];
        let all = find_gd_files_from_paths(&paths, &[], &root);
        let kept = find_gd_files_from_paths(&paths, &["addons".to_owned()], &root);
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            all.iter().filter(|f| f.ends_with(".gd")).count(),
            2,
            "{all:?}"
        );
        assert_eq!(kept, ["scripts/a.gd", "loose.txt"]);
    }
}
