//! Port of lark's `Indenter` with gdtoolkit's `GDScriptIndenter` (`gdscript_indenter.py`) on top: the
//! post-lexer that turns `_NL` tokens into `_INDENT`/`_DEDENT`, including inside brackets for
//! multiline lambdas.
//!
//! Python runs this as nested generators. Here one raw token is processed at a time into a queue, which
//! is equivalent: the indenter's state is read by nothing else, and the lexer is only asked for the next
//! token once the parser has taken everything the previous one produced. Errors are queued behind the
//! tokens Python yields before raising them, so a parser error on those tokens still comes first.

use std::collections::VecDeque;

use crate::error::{ErrorKind, ParseError};
use crate::lexer::{LexToken, Lexer, RawSpan};
use crate::tables::{Grammar, Symbol};

const TAB_LEN: usize = 4;
const BORROWED_VALUE: &str = "N/A";

/// The indenter's terminals; None for those a grammar lacks (comments.lark has only COMMENT).
struct Types {
    nl: Option<Symbol>,
    indent: Option<Symbol>,
    dedent: Option<Symbol>,
    open_paren: Vec<Symbol>,
    close_paren: Vec<Symbol>,
    lambda_separator: Vec<Symbol>,
    colon: Option<Symbol>,
    lpar: Option<Symbol>,
    rpar: Option<Symbol>,
    func: Option<Symbol>,
    type_hint: Option<Symbol>,
    name: Option<Symbol>,
}

impl Types {
    fn new(grammar: &Grammar) -> Types {
        let all = |names: &[&str]| names.iter().filter_map(|n| grammar.symbol(n)).collect();
        Types {
            nl: grammar.symbol("_NL"),
            indent: grammar.symbol("_INDENT"),
            dedent: grammar.symbol("_DEDENT"),
            open_paren: all(&["LPAR", "LSQB", "LBRACE"]),
            close_paren: all(&["RPAR", "RSQB", "RBRACE"]),
            lambda_separator: all(&["COMMA"]),
            colon: grammar.symbol("COLON"),
            lpar: grammar.symbol("LPAR"),
            rpar: grammar.symbol("RPAR"),
            func: grammar.symbol("FUNC"),
            type_hint: grammar.symbol("TYPE_HINT"),
            name: grammar.symbol("NAME"),
        }
    }
}

fn is(kind: Symbol, wanted: Option<Symbol>) -> bool {
    Some(kind) == wanted
}

/// What `_current_token_is_just_after_lambda_header` reads of each token it looks back on.
#[derive(Clone, Copy)]
struct Processed {
    kind: Symbol,
    is_arrow: bool,
}

pub struct Indenter<'s> {
    types: Types,
    paren_level: usize,
    indent_level: Vec<usize>,
    processed_tokens: Vec<Processed>,
    undedented_lambdas_at_paren_level: Vec<usize>,
    had_newline: bool,
    queue: VecDeque<LexToken<'s>>,
    pending_error: Option<ParseError>,
    /// The raw token whose bracket counting lark's `_process` does once its outputs are consumed.
    pending_paren: Option<Symbol>,
    finished: bool,
}

impl<'s> Indenter<'s> {
    pub fn new(grammar: &Grammar) -> Indenter<'s> {
        Indenter {
            types: Types::new(grammar),
            paren_level: 0,
            indent_level: vec![0],
            processed_tokens: Vec::new(),
            undedented_lambdas_at_paren_level: Vec::new(),
            had_newline: false,
            queue: VecDeque::new(),
            pending_error: None,
            pending_paren: None,
            finished: false,
        }
    }

    /// The next token for the parser, lexing in parser state `state` when the queue is empty.
    pub fn next(
        &mut self,
        lexer: &mut Lexer<'s>,
        state: u16,
    ) -> Result<Option<LexToken<'s>>, ParseError> {
        loop {
            if let Some(token) = self.queue.pop_front() {
                return Ok(Some(token));
            }
            if let Some(error) = self.pending_error.take() {
                return Err(error);
            }
            if self.finished {
                return Ok(None);
            }
            self.pull(lexer, state)?;
        }
    }

    /// One more raw token through the indenter: count the previous one's brackets, then lex and process
    /// the next, or finish at the end of the input.
    fn pull(&mut self, lexer: &mut Lexer<'s>, state: u16) -> Result<(), ParseError> {
        if let Some(kind) = self.pending_paren.take() {
            self.count_paren(kind)?;
        }
        match lexer.lex(state)? {
            Some(token) => self.process(token),
            None => self.finish(),
        }
        Ok(())
    }

    fn count_paren(&mut self, kind: Symbol) -> Result<(), ParseError> {
        if self.types.open_paren.contains(&kind) {
            self.paren_level += 1;
        } else if self.types.close_paren.contains(&kind) {
            self.paren_level = self.paren_level.checked_sub(1).ok_or(ParseError {
                line: None,
                column: None,
                kind: ErrorKind::Indenter("AssertionError: unbalanced closing bracket"),
            })?;
        }
        Ok(())
    }

    /// One raw token through `_record_stream`, lark's `_process`, and gdtoolkit's `_process`.
    fn process(&mut self, token: LexToken<'s>) {
        self.processed_tokens.push(Processed {
            kind: token.kind,
            is_arrow: token.value == "->",
        });
        if is(token.kind, self.types.nl) {
            if let Err(error) = self.handle_nl(token) {
                self.pending_error = Some(error);
            }
        } else {
            self.emit(token);
        }
        self.pending_paren = Some(token.kind);
    }

    /// The end of lark's `_process`: close every open indentation.
    fn finish(&mut self) {
        while self.indent_level.len() > 1 {
            self.indent_level.pop();
            let dedent = LexToken {
                kind: self.dedent(),
                value: "",
                span: RawSpan::default(),
            };
            self.emit(dedent);
        }
        self.finished = true;
    }

    /// gdtoolkit's `_process` loop body: before a closing bracket or comma, dedent every lambda still
    /// open at this bracket level.
    fn emit(&mut self, token: LexToken<'s>) {
        let closes = self.types.close_paren.contains(&token.kind)
            || self.types.lambda_separator.contains(&token.kind);
        if closes {
            while self.undedented_at_level() > 0 {
                self.dedent_lambda_at_token(token);
                self.had_newline = false;
            }
        }
        self.had_newline = is(token.kind, self.types.nl);
        self.queue.push_back(token);
    }

    fn dedent(&self) -> Symbol {
        self.types.dedent.expect("only grammars with _NL indent")
    }

    fn undedented_at_level(&self) -> usize {
        let level = self.paren_level;
        self.undedented_lambdas_at_paren_level
            .get(level)
            .copied()
            .unwrap_or(0)
    }

    fn add_undedented(&mut self, delta: isize) {
        let level = self.paren_level;
        let counts = &mut self.undedented_lambdas_at_paren_level;
        if counts.len() <= level {
            counts.resize(level + 1, 0);
        }
        counts[level] = counts[level].wrapping_add_signed(delta);
    }

    /// The DEDENT gdtoolkit makes in place of lark's: the newline's line, no column or position.
    fn line_dedent(&self, newline: &LexToken<'s>) -> LexToken<'s> {
        LexToken {
            kind: self.dedent(),
            value: "",
            span: RawSpan {
                line: newline.span.line,
                end_line: newline.span.line,
                ..RawSpan::default()
            },
        }
    }

    fn handle_nl(&mut self, token: LexToken<'s>) -> Result<(), ParseError> {
        if self.paren_level > 0 {
            return self.handle_nl_in_parens(token);
        }
        // lark's handle_NL, with gdtoolkit's DEDENT replacement and the extra NL after each DEDENT.
        self.emit(token);
        let indent = indentation(&token)?;
        let top = self.top();
        if indent > top {
            self.indent_level.push(indent);
            let indent_token = borrow(self.types.indent.expect("grammar has _INDENT"), &token);
            self.emit(LexToken {
                value: indent_str(&token)?,
                ..indent_token
            });
            return Ok(());
        }
        while indent < self.top() {
            self.indent_level.pop();
            self.emit(self.line_dedent(&token));
            self.emit(token);
        }
        if indent != self.top() {
            return Err(ParseError {
                line: None,
                column: None,
                kind: ErrorKind::Dedent {
                    indent,
                    expected: self.top(),
                },
            });
        }
        Ok(())
    }

    fn handle_nl_in_parens(&mut self, token: LexToken<'s>) -> Result<(), ParseError> {
        let indent = indentation(&token)?;
        if indent > self.top()
            && (self.current_token_is_just_after_lambda_header() || self.in_multiline_lambda())
        {
            self.indent_level.push(indent);
            self.add_undedented(1);
            self.emit(token);
            let indent_token = borrow(self.types.indent.expect("grammar has _INDENT"), &token);
            self.emit(LexToken {
                value: indent_str(&token)?,
                ..indent_token
            });
        } else if indent <= self.top() && self.in_multiline_lambda() {
            self.emit(token);
            self.dedent_lambdas_to(indent, token);
        }
        // Otherwise do nothing, as other expressions don't handle newlines.
        Ok(())
    }

    /// Close the multiline lambdas indented deeper than `indent`, each DEDENT followed by another NL while
    /// a lambda is still open.
    fn dedent_lambdas_to(&mut self, indent: usize, token: LexToken<'s>) {
        while indent < self.top() && self.in_multiline_lambda() {
            self.indent_level.pop();
            self.add_undedented(-1);
            self.emit(self.line_dedent(&token));
            if self.in_multiline_lambda() {
                self.emit(token);
            }
        }
    }

    fn top(&self) -> usize {
        *self
            .indent_level
            .last()
            .expect("indent_level starts at [0]")
    }

    fn in_multiline_lambda(&self) -> bool {
        self.undedented_at_level() > 0
    }

    fn dedent_lambda_at_token(&mut self, token: LexToken<'s>) {
        self.indent_level.pop();
        self.add_undedented(-1);
        if !self.had_newline {
            let nl = self.types.nl.expect("grammar has _NL");
            self.queue.push_back(LexToken {
                value: BORROWED_VALUE,
                ..borrow(nl, &token)
            });
        }
        self.queue.push_back(LexToken {
            value: BORROWED_VALUE,
            ..borrow(self.dedent(), &token)
        });
    }

    /// Looking back from the newline: `func [NAME] ( … ) [-> TYPE_HINT] :` with only newlines between.
    fn current_token_is_just_after_lambda_header(&self) -> bool {
        let mut extra_rpars = 0usize;
        let mut i = 0;
        for token in self.processed_tokens.iter().rev() {
            if i >= 4 {
                return true;
            }
            if self.header_part(i, token.kind, extra_rpars) {
                i += 1;
                continue;
            }
            if !self.may_skip(i, token, &mut extra_rpars) {
                return false;
            }
        }
        i >= 4
    }

    /// gdtoolkit's `pattern_functions[i]`: whether `kind` is the `i`th token of the header, from the end.
    fn header_part(&self, i: usize, kind: Symbol, extra_rpars: usize) -> bool {
        let t = &self.types;
        match i {
            0 => is(kind, t.colon),
            1 => is(kind, t.rpar),
            2 => is(kind, t.lpar) && extra_rpars == 0,
            _ => is(kind, t.func),
        }
    }

    /// gdtoolkit's `accept_functions[i]`: whether `token` may sit between the header's `i`th and next part.
    fn may_skip(&self, i: usize, token: &Processed, extra_rpars: &mut usize) -> bool {
        let t = &self.types;
        let kind = token.kind;
        match i {
            0 => is(kind, t.nl),
            1 => is(kind, t.nl) || is(kind, t.type_hint) || token.is_arrow,
            2 => self.lpar_accept(kind, extra_rpars),
            _ => is(kind, t.nl) || is(kind, t.name),
        }
    }

    /// gdtoolkit's `lpar_accept_function`: inside the parameters, track brackets nested in them.
    fn lpar_accept(&self, kind: Symbol, extra_rpars: &mut usize) -> bool {
        if is(kind, self.types.rpar) {
            *extra_rpars += 1;
        } else if is(kind, self.types.lpar) {
            if *extra_rpars == 0 {
                return false;
            }
            *extra_rpars -= 1;
        }
        true
    }
}

/// `Token.new_borrow_pos`: a new type with `token`'s value and positions.
fn borrow<'s>(kind: Symbol, token: &LexToken<'s>) -> LexToken<'s> {
    LexToken { kind, ..*token }
}

/// `token.rsplit('\n', 1)[1]`: the tabs and spaces after the newline's last line break. Python raises
/// IndexError when there is none.
fn indent_str<'s>(token: &LexToken<'s>) -> Result<&'s str, ParseError> {
    match token.value.rfind('\n') {
        Some(at) => Ok(&token.value[at + 1..]),
        None => Err(ParseError {
            line: None,
            column: None,
            kind: ErrorKind::Indenter("IndexError: list index out of range"),
        }),
    }
}

fn indentation(token: &LexToken<'_>) -> Result<usize, ParseError> {
    let text = indent_str(token)?;
    let spaces = text.bytes().filter(|&b| b == b' ').count();
    let tabs = text.bytes().filter(|&b| b == b'\t').count();
    Ok(spaces + tabs * TAB_LEN)
}
