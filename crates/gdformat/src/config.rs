//! The `gdformatrc` config gdformat finds above the working directory, as `__main__.py` reads it.

use std::path::{Path, PathBuf};

use yaml_rust2::{Yaml, YamlLoader};

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
    let documents = YamlLoader::load_from_str(text).map_err(|e| e.to_string())?;
    let Some(Yaml::Hash(entries)) = documents.into_iter().next() else {
        return Err("the config is not a mapping".to_owned());
    };
    let mut config = Config::default();
    for (key, value) in entries {
        match key.as_str() {
            Some("excluded_directories") => config.excluded_directories = string_set(&value)?,
            Some("safety_checks") => config.safety_checks = truthy(&value),
            Some("use_spaces") => config.use_spaces = optional_count(&value)?,
            Some("line_length") => {
                config.line_length = value.as_i64().ok_or("line_length is not an integer")?
            }
            _ => {}
        }
    }
    Ok(config)
}

/// `set(value)` of a list, or of a mapping's keys, as `yaml.dump` writes a set.
fn string_set(value: &Yaml) -> Result<Vec<String>, String> {
    let items: Vec<&Yaml> = match value {
        Yaml::Array(items) => items.iter().collect(),
        Yaml::Hash(entries) => entries.keys().collect(),
        _ => return Err("excluded_directories is not a list".to_owned()),
    };
    items
        .into_iter()
        .map(|item| {
            scalar_text(item).ok_or_else(|| "an excluded directory is not a string".to_owned())
        })
        .collect()
}

fn scalar_text(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(text) | Yaml::Real(text) => Some(text.clone()),
        Yaml::Integer(n) => Some(n.to_string()),
        Yaml::Boolean(b) => Some(if *b { "True" } else { "False" }.to_owned()),
        _ => None,
    }
}

fn truthy(value: &Yaml) -> bool {
    match value {
        Yaml::Null | Yaml::BadValue => false,
        Yaml::Boolean(b) => *b,
        Yaml::Integer(n) => *n != 0,
        Yaml::Real(text) => text.parse::<f64>().is_ok_and(|x| x != 0.0),
        Yaml::String(text) => !text.is_empty(),
        Yaml::Array(items) => !items.is_empty(),
        Yaml::Hash(entries) => !entries.is_empty(),
        Yaml::Alias(_) => true,
    }
}

fn optional_count(value: &Yaml) -> Result<Option<usize>, String> {
    match value {
        Yaml::Null => Ok(None),
        Yaml::Integer(n) if *n > 0 => Ok(Some(*n as usize)),
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
}
