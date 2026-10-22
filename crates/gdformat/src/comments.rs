use std::collections::HashMap;

use gdscript::{Token, Tree};

use pycompat::string;

fn comment_tokens(comment_parse_tree: &Tree) -> impl Iterator<Item = &Token> {
    comment_parse_tree
        .children
        .iter()
        .map(|child| child.as_token().expect("comment tree holds only tokens"))
}

pub fn gather_comments(comment_parse_tree: &Tree) -> Vec<String> {
    comment_tokens(comment_parse_tree)
        .map(|comment| string::rstrip(&comment.value).to_owned())
        .collect()
}

/// For each line of the code, 1-based (index 0 is None), the comment that's alone on it.
pub fn gather_standalone_comments(
    gdscript_code: &str,
    comment_parse_tree: &Tree,
) -> Vec<Option<String>> {
    gather_comments_by_prefix(gdscript_code, comment_parse_tree, |prefix| {
        prefix.chars().all(string::is_space)
    })
}

/// For each line of the code, 1-based (index 0 is None), the comment that follows code on it.
pub fn gather_inline_comments(
    gdscript_code: &str,
    comment_parse_tree: &Tree,
) -> Vec<Option<String>> {
    gather_comments_by_prefix(gdscript_code, comment_parse_tree, |prefix| {
        prefix.chars().any(|c| !string::is_space(c))
    })
}

/// The comment of each line whose prefix, all the line's characters before the comment, passes
/// `prefix_matches`; rstripped.
fn gather_comments_by_prefix(
    gdscript_code: &str,
    comment_parse_tree: &Tree,
    prefix_matches: impl Fn(&str) -> bool,
) -> Vec<Option<String>> {
    let line_to_comment_mapping: HashMap<usize, &Token> = comment_tokens(comment_parse_tree)
        .map(|comment| (comment.span.line, comment))
        .collect();
    let mut comments = vec![None];
    for (line_number, line) in string::splitlines(gdscript_code).into_iter().enumerate() {
        let comment = line_to_comment_mapping
            .get(&(line_number + 1))
            .filter(|comment| {
                let prefix: String = line.chars().take(comment.span.column - 1).collect();
                prefix_matches(&prefix)
            });
        comments.push(comment.map(|comment| string::rstrip(&comment.value).to_owned()));
    }
    comments
}
