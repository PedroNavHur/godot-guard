pub const TAB_INDENT_SIZE: usize = 4;
pub const INLINE_COMMENT_OFFSET: usize = 2;

/// Empty lines forced around these statements, by statement name; None means no rule.
pub type SurroundingEmptyLinesTable = fn(&str) -> Option<usize>;

const FUNCTION_LIKE: [&str; 9] = [
    "class_def",
    "func_def",
    "static_func_def",
    "remote_func_def",
    "remotesync_func_def",
    "master_func_def",
    "mastersync_func_def",
    "puppet_func_def",
    "sync_func_def",
];

pub fn default_surrounding_empty_lines(statement: &str) -> Option<usize> {
    FUNCTION_LIKE.contains(&statement).then_some(1)
}

pub fn global_scope_surrounding_empty_lines(statement: &str) -> Option<usize> {
    FUNCTION_LIKE.contains(&statement).then_some(2)
}
