//! The pattern rules' names: the per-file counts the baseline records, and the advisory warnings.
//!
//! ceiling: these mirror `patterns.py`'s `COUNTED` and `ADVISORY` until `gdrules::patterns` lands on
//! rust-port; then this module re-exports them from there.

/// Baselined per file, counted like long-comment: (rule, the noun its count labels).
pub const COUNTED: &[(&str, &str)] = &[
    ("lookup-in-hot-path", "hot-path lookups"),
    ("load-in-hot-path", "hot-path loads"),
    ("private-export", "private exports"),
    ("self-assign", "self-assignments"),
];

pub const ADVISORY: &[&str] = &["per-frame-allocation"];
