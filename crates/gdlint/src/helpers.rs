//! `helpers.py`.

pub fn is_function_public(function_name: &str) -> bool {
    !function_name.starts_with('_')
}
