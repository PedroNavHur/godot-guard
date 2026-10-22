pub type PreviouslyProcessedLineNumber = i64;
pub type FormattedLine = (Option<i64>, String);
pub type FormattedLines = Vec<FormattedLine>;
pub type Outcome = (FormattedLines, PreviouslyProcessedLineNumber);
