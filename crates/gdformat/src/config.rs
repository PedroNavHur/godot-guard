//! The `gdformatrc` config gdformat finds above the working directory, as `__main__.py` reads it.

use std::path::{Path, PathBuf};

use pycompat::yaml::{self, Value};

pub const CONFIG_FILE_NAME: &str = "gdformatrc";

/// `yaml.dump(DEFAULT_CONFIG)`, what `--dump-default-config` writes.
pub const DEFAULT_CONFIG_YAML: &str = "excluded_directories: !!set\n  .git: null\nline_length: 100\nsafety_checks: null\nuse_spaces: null\n";

/// The entries gdformat reads, defaults filled in for the missing ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Directory names `find_gd_files_from_paths` doesn't descend into.
    pub excluded_directories: Vec<String>,
    /// The entry's Python truthiness: gdformat runs the checks when it is truthy, and the default, None,
    /// is not.
    pub safety_checks: bool,
    pub use_spaces: Option<usize>,
    pub line_length: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            excluded_directories: vec![".git".to_owned()],
            safety_checks: false,
            use_spaces: None,
            line_length: 100,
        }
    }
}

/// `_find_config_file`: `gdformatrc`, then `.gdformatrc`, in `start` and each parent, stopping short of
/// the filesystem root.
pub fn find_config_file(start: &Path) -> Option<PathBuf> {
    let mut search_dir = Some(start);
    while let Some(dir) = search_dir.filter(|dir| dir.parent().is_some()) {
        for name in [CONFIG_FILE_NAME.to_owned(), format!(".{CONFIG_FILE_NAME}")] {
            let file_path = dir.join(name);
            if file_path.is_file() {
                return Some(file_path);
            }
        }
        search_dir = dir.parent();
    }
    None
}

/// The config at `path`, or the defaults when there is none.
pub fn load_config(path: Option<&Path>) -> Result<Config, String> {
    let Some(path) = path else {
        return Ok(Config::default());
    };
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_config(&text).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn parse_config(text: &str) -> Result<Config, String> {
    let Some(Value::Dict(entries)) = yaml::load(text).map_err(|e| e.to_string())? else {
        return Err("the config is not a mapping".to_owned());
    };
    let mut config = Config::default();
    for (key, value) in entries {
        let Value::Str(name) = key else { continue };
        read_entry(&mut config, &name, &value)?;
    }
    Ok(config)
}

/// The entry `name`, if gdformat reads it, as `__main__.py` uses its value.
fn read_entry(config: &mut Config, name: &str, value: &Value) -> Result<(), String> {
    match name {
        "excluded_directories" => config.excluded_directories = string_set(value)?,
        "safety_checks" => config.safety_checks = truthy(value),
        "use_spaces" => config.use_spaces = optional_count(value)?,
        "line_length" => config.line_length = integer(value)?,
        _ => {}
    }
    Ok(())
}

/// `set(value)` of a list, a set, or a mapping's keys, as `yaml.dump` writes a set, keeping the strings:
/// no other scalar equals a directory name, and `set()` raises on a nested collection.
fn string_set(value: &Value) -> Result<Vec<String>, String> {
    let items: Vec<&Value> = match value {
        Value::List(items) | Value::Set(items) => items.iter().collect(),
        Value::Dict(entries) => entries.iter().map(|(key, _)| key).collect(),
        _ => return Err("excluded_directories is not a list".to_owned()),
    };
    let mut names = Vec::new();
    for item in items {
        match item {
            Value::Str(name) => names.push(name.clone()),
            Value::List(_) | Value::Dict(_) | Value::Set(_) => {
                return Err("an excluded directory is not a string".to_owned());
            }
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => {}
        }
    }
    Ok(names)
}

/// Python's truthiness of a value PyYAML built.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Float(x) => *x != 0.0,
        Value::Str(text) => !text.is_empty(),
        Value::List(items) | Value::Set(items) => !items.is_empty(),
        Value::Dict(entries) => !entries.is_empty(),
    }
}

/// `int` of a number PyYAML built; a bool is the int it subclasses.
fn integer(value: &Value) -> Result<i64, String> {
    match value {
        Value::Int(n) => Ok(*n),
        Value::Bool(b) => Ok(i64::from(*b)),
        _ => Err("line_length is not an integer".to_owned()),
    }
}

fn optional_count(value: &Value) -> Result<Option<usize>, String> {
    match value {
        Value::Null => Ok(None),
        Value::Int(n) if *n > 0 => Ok(Some(*n as usize)),
        _ => Err("use_spaces is neither null nor a positive integer".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dump_reads_as_the_defaults() {
        assert_eq!(
            parse_config(DEFAULT_CONFIG_YAML).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn missing_entries_take_the_defaults() {
        let config = parse_config("line_length: 80\nuse_spaces: 2\nsafety_checks: true\n").unwrap();
        assert_eq!(
            config,
            Config {
                line_length: 80,
                use_spaces: Some(2),
                safety_checks: true,
                ..Config::default()
            }
        );
    }

    #[test]
    fn excluded_directories_read_as_a_list_or_a_set() {
        let listed = parse_config("excluded_directories: [addons, .git]\n").unwrap();
        assert_eq!(listed.excluded_directories, ["addons", ".git"]);
        let set = parse_config("excluded_directories: !!set\n  addons: null\n").unwrap();
        assert_eq!(set.excluded_directories, ["addons"]);
    }

    #[test]
    fn only_string_entries_can_name_an_excluded_directory() {
        let config = parse_config("excluded_directories: [a, null, 1]\n").unwrap();
        assert_eq!(config.excluded_directories, ["a"]);
        assert!(parse_config("excluded_directories: [a, [b]]\n").is_err());
    }

    #[test]
    fn yaml_1_1_words_read_as_pyyaml_reads_them() {
        let config = parse_config("safety_checks: off\nline_length: 1_000\n").unwrap();
        assert!(!config.safety_checks);
        assert_eq!(config.line_length, 1000);
        assert!(parse_config("safety_checks: yes\n").unwrap().safety_checks);
        assert!(!parse_config("safety_checks: no\n").unwrap().safety_checks);
    }

    #[test]
    fn a_line_length_that_is_not_an_integer_is_refused() {
        assert_eq!(
            parse_config("line_length: wide\n"),
            Err("line_length is not an integer".to_owned())
        );
    }
}
