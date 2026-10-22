use crate::annotation::{
    is_abstract_annotation_for_statement, is_non_standalone_annotation,
    prepend_annotations_to_formatted_line,
};
use crate::constants::{SurroundingEmptyLinesTable, TAB_INDENT_SIZE};
use crate::context::Context;
use crate::function_statement::StatementFormatter;
use crate::node::Ex;
use crate::py;
use crate::types::{FormattedLines, Outcome};

pub fn format_block<'a>(
    statements: &[Ex<'a>],
    statement_formatter: StatementFormatter,
    context: &mut Context<'a>,
    surrounding_empty_lines_table: SurroundingEmptyLinesTable,
) -> Outcome {
    let mut previous_statement_name: Option<&'static str> = None;
    let mut formatted_lines = FormattedLines::new();
    let mut previously_processed_line_number = context.previously_processed_line_number;

    for (i, statement) in statements.iter().enumerate() {
        let blank_lines = |context: &Context| {
            blank_lines_before(
                previous_statement_name,
                statement,
                previously_processed_line_number,
                context,
                surrounding_empty_lines_table,
            )
        };

        if is_pending_annotation(statements, i, statement_formatter) {
            context.annotations.push(statement.clone());
            // Handle first annotation case
            if context.annotations.len() == 1 {
                formatted_lines.extend(blank_lines(context));
            }
            continue;
        }

        if context.annotations.is_empty() {
            formatted_lines.extend(blank_lines(context));
        }

        let (lines, last_line) = statement_formatter.format(statement, context);
        previously_processed_line_number = last_line;
        formatted_lines.extend(with_annotations(lines, context));
        previous_statement_name = Some(statement.data());
    }

    // Handle end of block
    let dedent_line_number = find_dedent_line_number(previously_processed_line_number, context);
    formatted_lines.extend(remove_empty_strings_from_end(
        reconstruct_blank_lines_in_range(
            previously_processed_line_number,
            dedent_line_number,
            context,
        ),
    ));
    (formatted_lines, dedent_line_number - 1)
}

fn is_non_standalone_annotation_at_given_scope(
    statement: &Ex,
    statement_formatter: StatementFormatter,
) -> bool {
    if !is_non_standalone_annotation(statement) {
        return false;
    }
    !(statement_formatter == StatementFormatter::Func
        && statement.child(0).value() == "warning_ignore")
}

fn is_pending_annotation(
    statements: &[Ex],
    index: usize,
    statement_formatter: StatementFormatter,
) -> bool {
    let statement = &statements[index];
    // Check if this is an abstract annotation followed by an abstract function or class_name
    let is_abstract_for_statement = statements
        .get(index + 1)
        .is_some_and(|next| is_abstract_annotation_for_statement(statement, next));
    is_non_standalone_annotation_at_given_scope(statement, statement_formatter)
        || is_abstract_for_statement
}

fn blank_lines_before(
    previous_statement_name: Option<&str>,
    statement: &Ex,
    previously_processed_line_number: i64,
    context: &Context,
    surrounding_empty_lines_table: SurroundingEmptyLinesTable,
) -> FormattedLines {
    let blank_lines = reconstruct_blank_lines_in_range(
        previously_processed_line_number,
        statement.line(),
        context,
    );
    let Some(previous) = previous_statement_name else {
        return remove_empty_strings_from_begin(blank_lines);
    };
    let blank_lines = add_extra_blanks_due_to_previous_statement(
        blank_lines,
        previous,
        surrounding_empty_lines_table,
    );
    add_extra_blanks_due_to_next_statement(
        blank_lines,
        statement.data(),
        surrounding_empty_lines_table,
    )
}

fn with_annotations<'a>(mut lines: FormattedLines, context: &mut Context<'a>) -> FormattedLines {
    if context.annotations.is_empty() {
        return lines;
    }
    let rest = lines.split_off(1);
    let mut annotated = prepend_annotations_to_formatted_line(lines.remove(0), context);
    annotated.extend(rest);
    annotated
}

pub fn reconstruct_blank_lines_in_range(begin: i64, end: i64, context: &Context) -> FormattedLines {
    let comments = context.standalone_comments;
    let comments_in_range = &comments[py::slice(comments.len(), Some(begin + 1), Some(end))];
    let line_count = (end - begin - 1).max(0) as usize;
    let mut reconstructed_lines = Vec::new();
    for (line_no, comment) in (begin + 1..end).zip(comments_in_range) {
        match comment {
            Some(comment) => {
                let prefix = match context.gdscript_code_lines[line_no as usize].starts_with('#') {
                    true => "",
                    false => context.indent_string.as_str(),
                };
                reconstructed_lines.push(format!("{prefix}{comment}"));
            }
            None => reconstructed_lines.push(String::new()),
        }
    }
    squeeze_lines(reconstructed_lines)
        .into_iter()
        .take(line_count)
        .map(|line| (None, line))
        .collect()
}

/// The `^\t{0,N}[^\t]+` test of `_find_dedent_line_number`: at most N leading tabs, then something
/// else. The Python computes N with true division and `%d` truncates it.
fn tab_dedent_matches(line: &str, indent: usize) -> bool {
    let most = (indent as f64 / TAB_INDENT_SIZE as f64 - 1.0).trunc() as i64;
    let leading = line.len() - line.trim_start_matches('\t').len();
    leading as i64 <= most && leading < line.len()
}

/// The `^ {0,N}[^ ]+` test, N being one less than the indent: fewer leading spaces, then something else.
fn space_dedent_matches(line: &str, indent: usize) -> bool {
    let leading = line.len() - line.trim_start_matches(' ').len();
    leading < indent && leading < line.len()
}

/// The three break tests of `_find_dedent_line_number`'s forward scan, in the Python's order.
fn ends_block(line: &str, indent: usize) -> bool {
    if line.starts_with(' ') && space_dedent_matches(line, indent) {
        return true;
    }
    if line.starts_with('\t') && tab_dedent_matches(line, indent) {
        return true;
    }
    indent > 0 && !line.is_empty() && !line.starts_with([' ', '\t'])
}

// TODO: indent detection & refactoring
fn find_dedent_line_number(previously_processed_line_number: i64, context: &Context) -> i64 {
    let lines = context.gdscript_code_lines;
    if previously_processed_line_number == lines.len() as i64 - 1 || context.indent == 0 {
        return lines.len() as i64;
    }
    let first = previously_processed_line_number + 1;
    let following = &lines[py::slice(lines.len(), Some(first), None)];
    let line_no = first
        + following
            .iter()
            .take_while(|line| !ends_block(line, context.indent))
            .count() as i64;
    // `lines[line_no - 1 :: -1]`
    let start = (line_no - 1).min(lines.len() as i64 - 1);
    let blank_run = (0..=start)
        .rev()
        .take_while(|&index| py::strip(lines[index as usize]).is_empty())
        .count();
    line_no - blank_run as i64
}

fn add_extra_blanks_due_to_previous_statement(
    blank_lines: FormattedLines,
    previous_statement_name: &str,
    surrounding_empty_lines_table: SurroundingEmptyLinesTable,
) -> FormattedLines {
    // assumption: there is no sequence of empty lines longer than 1 (in blank lines)
    let Some(forced_blanks_num) = surrounding_empty_lines_table(previous_statement_name) else {
        return blank_lines;
    };
    let starts_blank = blank_lines.first().is_some_and(|(_, line)| line.is_empty());
    let lines_to_prepend = forced_blanks_num as i64 - i64::from(starts_blank);
    let mut lines = empty_lines(lines_to_prepend);
    lines.extend(blank_lines);
    lines
}

fn add_extra_blanks_due_to_next_statement(
    mut blank_lines: FormattedLines,
    next_statement_name: &str,
    surrounding_empty_lines_table: SurroundingEmptyLinesTable,
) -> FormattedLines {
    // assumption: there is no sequence of empty lines longer than 2 (in blank lines)
    let Some(forced_blanks_num) = surrounding_empty_lines_table(next_statement_name) else {
        return blank_lines;
    };
    let first_empty_line_ix_from_end = blank_lines.iter().rposition(|(_, line)| line.is_empty());
    let mut empty_lines_already_in_place = i64::from(first_empty_line_ix_from_end.is_some());
    if let Some(ix) = first_empty_line_ix_from_end
        && ix > 0
        && blank_lines[ix - 1].1.is_empty()
    {
        empty_lines_already_in_place += 1;
    }
    let lines_to_inject = empty_lines(forced_blanks_num as i64 - empty_lines_already_in_place);
    let at = first_empty_line_ix_from_end.unwrap_or(0);
    blank_lines.splice(at..at, lines_to_inject);
    blank_lines
}

/// `n * [(None, "")]`, which is empty for n <= 0.
fn empty_lines(n: i64) -> FormattedLines {
    (0..n.max(0)).map(|_| (None, String::new())).collect()
}

fn squeeze_lines(lines: Vec<String>) -> Vec<String> {
    let mut squeezed_lines = Vec::new();
    let mut previous_line_empty = false;
    for line in lines {
        let empty = line.is_empty();
        if !empty || !previous_line_empty {
            squeezed_lines.push(line);
        }
        previous_line_empty = empty;
    }
    squeezed_lines
}

fn remove_empty_strings_from_begin(lst: FormattedLines) -> FormattedLines {
    match lst.iter().position(|(_, line)| !line.is_empty()) {
        Some(i) => lst.into_iter().skip(i).collect(),
        None => Vec::new(),
    }
}

fn remove_empty_strings_from_end(mut lst: FormattedLines) -> FormattedLines {
    let keep = lst
        .iter()
        .rposition(|(_, line)| !line.is_empty())
        .map_or(0, |i| i + 1);
    lst.truncate(keep);
    lst
}
