//! Port of lark 1.2.2's LALR runtime: `parse_from_state` (`lalr_parser.py`) and `feed_token`
//! (`lalr_parser_state.py`), with the post-lexer between the contextual lexer and the parser.

use crate::error::{ErrorKind, ParseError};
use crate::indenter::Indenter;
use crate::lexer::{Language, LexToken, Lexer, RawSpan};
use crate::tables::{Action, Grammar};
use crate::tree::{Node, Tree};
use crate::tree_builder::{Item, reduce};

pub fn parse(language: &'static Language, text: &str) -> Result<Tree, ParseError> {
    let grammar = language.grammar;
    let mut parser = ParserState {
        language,
        grammar,
        state_stack: vec![grammar.start_state],
        value_stack: Vec::new(),
    };
    let mut lexer = Lexer::new(language, text);
    let mut indenter = Indenter::new(grammar);
    let mut last: Option<LexToken<'_>> = None;
    while let Some(token) = indenter.next(&mut lexer, parser.position())? {
        parser.feed_token(token, false)?;
        last = Some(token);
    }
    let end = grammar.symbol("$END").expect("every grammar has $END");
    let end_token = match last {
        Some(token) => LexToken {
            kind: end,
            value: "",
            ..token
        },
        None => LexToken {
            kind: end,
            value: "",
            span: RawSpan {
                start_pos: Some(0),
                line: Some(1),
                column: Some(1),
                ..RawSpan::default()
            },
        },
    };
    let result = parser.feed_token(end_token, true)?;
    match result
        .expect("the end token completes the parse")
        .into_node(grammar)
    {
        Node::Tree(tree) => Ok(tree),
        Node::Token(token) => panic!("the start rule produced a token: {token:?}"),
    }
}

struct ParserState<'s> {
    language: &'static Language,
    grammar: &'static Grammar,
    state_stack: Vec<u16>,
    value_stack: Vec<Item<'s>>,
}

impl<'s> ParserState<'s> {
    fn position(&self) -> u16 {
        *self
            .state_stack
            .last()
            .expect("the stack holds the start state")
    }

    /// Reduce as far as `token` allows, then shift it; at the end, return the start rule's value.
    fn feed_token(
        &mut self,
        token: LexToken<'s>,
        is_end: bool,
    ) -> Result<Option<Item<'s>>, ParseError> {
        loop {
            let state = self.position();
            let action = self
                .language
                .action(state, token.kind)
                .ok_or_else(|| self.unexpected(state, &token))?;
            match action {
                Action::Shift(next) => {
                    assert!(!is_end, "lark never shifts $END");
                    self.state_stack.push(next);
                    self.value_stack.push(Item::Lexed(token));
                    return Ok(None);
                }
                Action::Reduce(index) => {
                    let next = self.reduce_and_goto(index);
                    if is_end && next == self.grammar.end_state {
                        return Ok(self.value_stack.pop());
                    }
                }
            }
        }
    }

    /// The error for `token` with no action in `state`.
    fn unexpected(&self, state: u16, token: &LexToken<'s>) -> ParseError {
        ParseError {
            line: token.span.line,
            column: token.span.column,
            kind: ErrorKind::UnexpectedToken {
                token: self.grammar.symbols[token.kind as usize],
                value: token.value.to_owned(),
                expected: self.grammar.expected(state),
            },
        }
    }

    /// Reduce by rule `index`, then push and return the state the rule's origin goes to.
    fn reduce_and_goto(&mut self, index: u16) -> u16 {
        let rule = &self.grammar.rules[index as usize];
        reduce(self.grammar, rule, &mut self.value_stack);
        self.state_stack
            .truncate(self.state_stack.len() - rule.size);
        let Some(Action::Shift(next)) = self.language.action(self.position(), rule.origin) else {
            panic!("no goto for {} from state {}", rule.data, self.position());
        };
        self.state_stack.push(next);
        next
    }
}
