use crate::node::Ex;

pub struct Context<'a> {
    pub single_indent: usize,
    pub single_indent_string: &'a str,
    pub indent: usize,
    pub indent_string: String,
    pub previously_processed_line_number: i64,
    pub max_line_length: i64,
    pub gdscript_code_lines: &'a [&'a str],
    pub standalone_comments: &'a [Option<String>],
    pub inline_comments: &'a [Option<String>],
    pub annotations: Vec<Ex<'a>>,
}

impl<'a> Context<'a> {
    pub fn new(
        single_indent: usize,
        single_indent_string: &'a str,
        max_line_length: i64,
        gdscript_code_lines: &'a [&'a str],
        standalone_comments: &'a [Option<String>],
        inline_comments: &'a [Option<String>],
    ) -> Self {
        Context {
            single_indent,
            single_indent_string,
            indent: 0,
            indent_string: String::new(),
            previously_processed_line_number: 0,
            max_line_length,
            gdscript_code_lines,
            standalone_comments,
            inline_comments,
            annotations: Vec::new(),
        }
    }

    pub fn create_child_context(&self, previously_processed_line_number: i64) -> Context<'a> {
        let indent = self.indent + self.single_indent;
        Context {
            indent,
            indent_string: self
                .single_indent_string
                .repeat(indent / self.single_indent),
            previously_processed_line_number,
            annotations: Vec::new(),
            ..*self
        }
    }

    /// The match of `^{single_indent_string[0]}+` in `line`: its leading run of the indent character.
    pub fn indent_match<'l>(&self, line: &'l str) -> &'l str {
        let indent_char = self
            .single_indent_string
            .chars()
            .next()
            .expect("empty indent string");
        let rest = line.trim_start_matches(indent_char);
        &line[..line.len() - rest.len()]
    }
}

#[derive(Clone, Debug)]
pub struct ExpressionContext {
    pub prefix_string: String,
    /// Earliest line number of the prefix string.
    pub prefix_line: i64,
    pub suffix_string: String,
    /// Earliest line number of the suffix string.
    pub suffix_line: i64,
}

impl ExpressionContext {
    pub fn new(
        prefix: impl Into<String>,
        prefix_line: i64,
        suffix: impl Into<String>,
        suffix_line: i64,
    ) -> Self {
        ExpressionContext {
            prefix_string: prefix.into(),
            prefix_line,
            suffix_string: suffix.into(),
            suffix_line,
        }
    }
}
