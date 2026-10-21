use gdscript::Tree;

use crate::block::format_block;
use crate::comments::{gather_inline_comments, gather_standalone_comments};
use crate::constants::{
    INLINE_COMMENT_OFFSET, TAB_INDENT_SIZE, global_scope_surrounding_empty_lines,
};
use crate::context::Context;
use crate::function_statement::StatementFormatter;
use crate::node::Ex;
use crate::py;
use crate::types::FormattedLines;

/// gdtoolkit's `format_code`, given the trees `parser.parse(code, gather_metadata=True)` and
/// `parser.parse_comments(code)` build. `spaces_for_indent` None indents with tabs.
pub fn format_code(
    gdscript_code: &str,
    max_line_length: i64,
    spaces_for_indent: Option<usize>,
    parse_tree: &Tree,
    comment_parse_tree: &Tree,
) -> String {
    let mut gdscript_code_lines = vec![""];
    gdscript_code_lines.extend(py::splitlines(gdscript_code));
    let single_indent_size = spaces_for_indent.unwrap_or(TAB_INDENT_SIZE);
    let single_indent_string = match spaces_for_indent {
        None => "\t".to_owned(),
        Some(spaces) => " ".repeat(spaces),
    };
    let standalone_comments = gather_standalone_comments(gdscript_code, comment_parse_tree);
    let inline_comments = gather_inline_comments(gdscript_code, comment_parse_tree);
    let mut context = Context::new(
        single_indent_size,
        &single_indent_string,
        max_line_length,
        &gdscript_code_lines,
        &standalone_comments,
        &inline_comments,
    );
    let statements: Vec<Ex> = parse_tree.children.iter().map(Ex::from).collect();
    let (mut formatted_lines, _) = format_block(
        &statements,
        StatementFormatter::Class,
        &mut context,
        global_scope_surrounding_empty_lines,
    );
    formatted_lines.push((None, String::new()));
    let formatted_lines = add_inline_comments(formatted_lines, context.inline_comments);
    let formatted_lines =
        add_standalone_comments(formatted_lines, context.standalone_comments, &context);
    formatted_lines
        .into_iter()
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn add_inline_comments(
    formatted_lines: FormattedLines,
    comments: &[Option<String>],
) -> FormattedLines {
    let mut remaining_comments = comments.to_vec();
    let mut postprocessed_lines = FormattedLines::new();
    let comment_offset = " ".repeat(INLINE_COMMENT_OFFSET);

    for (line_no, line) in formatted_lines.into_iter().rev() {
        let Some(number) = line_no else {
            postprocessed_lines.push((line_no, line));
            continue;
        };
        let start = py::slice(remaining_comments.len(), Some(number), None).start;
        let comments = remaining_comments.split_off(start);
        if comments.is_empty() {
            postprocessed_lines.push((line_no, line));
            continue;
        }
        let mut parts = vec![line];
        parts.extend(comments.into_iter().flatten());
        postprocessed_lines.push((line_no, parts.join(&comment_offset)));
    }
    postprocessed_lines.reverse();
    postprocessed_lines
}

fn add_standalone_comments(
    formatted_lines: FormattedLines,
    standalone_comments: &[Option<String>],
    context: &Context,
) -> FormattedLines {
    let mut remaining_comments = standalone_comments.to_vec();
    let mut postprocessed_lines = FormattedLines::new();
    let mut currently_inside_expression = false;
    let mut last_expression_line_no = 0;

    for (line_no, line) in formatted_lines.into_iter().rev() {
        let Some(number) = line_no else {
            postprocessed_lines.push((line_no, line));
            currently_inside_expression = false;
            continue;
        };
        if !currently_inside_expression {
            postprocessed_lines.push((line_no, line));
            currently_inside_expression = true;
            last_expression_line_no = number;
            continue;
        }
        let range = py::slice(
            remaining_comments.len(),
            Some(number),
            Some(last_expression_line_no),
        );
        let comments = remaining_comments[range.clone()].to_vec();
        remaining_comments.truncate(range.start);
        let previous = &postprocessed_lines[postprocessed_lines.len() - 1].1;
        let indent = get_greater_indent(&line, previous, context).to_owned();
        for comment in comments.into_iter().rev().flatten() {
            postprocessed_lines.push((None, format!("{indent}{comment}")));
        }
        postprocessed_lines.push((line_no, line));
    }
    postprocessed_lines.reverse();
    postprocessed_lines
}

fn get_greater_indent<'l>(line_a: &'l str, line_b: &'l str, context: &Context) -> &'l str {
    let line_a_indent = context.indent_match(line_a);
    let line_b_indent = context.indent_match(line_b);
    match py::len(line_a_indent) > py::len(line_b_indent) {
        true => line_a_indent,
        false => line_b_indent,
    }
}
