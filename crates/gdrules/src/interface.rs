//! The part of `interface.py` the tree rules share: which annotations export a variable.

/// Annotations that start with `export` but group the inspector instead of exporting a variable.
pub const NOT_VARIABLES: [&str; 3] = ["export_group", "export_subgroup", "export_category"];

/// Whether an annotation is an `@export` of a variable: group, subgroup, and category are not.
pub fn is_export_annotation(name: &str) -> bool {
    name.starts_with("export") && !NOT_VARIABLES.contains(&name)
}
