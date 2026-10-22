//! Port of lark 1.2.2's `lexer.py` runtime: `LineCounter`, `BasicLexer.next_token`, and
//! `ContextualLexer.lex`, which picks the lexer for the parser's current state.

use std::sync::OnceLock;

use crate::error::{ErrorKind, ParseError};
use crate::matchers::{self, NON_ASCII};
use crate::tables::{Action, DenseTable, Grammar, Pattern, Symbol};

/// A lark Token's positions. Each may be None: the indenter makes tokens without some or all of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawSpan {
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub end_line: Option<usize>,
    pub end_column: Option<usize>,
    pub start_pos: Option<usize>,
    pub end_pos: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub struct LexToken<'s> {
    pub kind: Symbol,
    pub value: &'s str,
    pub span: RawSpan,
}

/// The grammar's tables plus what the lexer derives from them once.
pub struct Language {
    pub grammar: &'static Grammar,
    compiled: OnceLock<Compiled>,
}

struct Compiled {
    table: DenseTable,
    /// Per lexer and first-byte class, the terminals that may match, in the lexer's order.
    candidates: Vec<Vec<Vec<Symbol>>>,
    ignore: Vec<bool>,
    newline: Vec<bool>,
}

impl Language {
    pub const fn new(grammar: &'static Grammar) -> Language {
        Language {
            grammar,
            compiled: OnceLock::new(),
        }
    }

    pub fn action(&self, state: u16, symbol: Symbol) -> Option<Action> {
        self.compiled().table.action(state, symbol)
    }

    fn compiled(&self) -> &Compiled {
        self.compiled.get_or_init(|| Compiled::new(self.grammar))
    }
}

impl Compiled {
    fn new(grammar: &Grammar) -> Compiled {
        let flags = |symbols: &[Symbol]| {
            let mut set = vec![false; grammar.terminal_count];
            symbols.iter().for_each(|&s| set[s as usize] = true);
            set
        };
        let candidates = grammar
            .lexers
            .iter()
            .map(|lexer| {
                (0..=NON_ASCII)
                    .map(|class| {
                        let starts = |&&symbol: &&Symbol| can_start(grammar, symbol, class);
                        lexer.order.iter().filter(starts).copied().collect()
                    })
                    .collect()
            })
            .collect();
        Compiled {
            table: DenseTable::new(grammar),
            candidates,
            ignore: flags(grammar.ignore),
            newline: flags(grammar.newline_types),
        }
    }
}

fn pattern(grammar: &Grammar, symbol: Symbol) -> &'static Pattern {
    grammar.patterns[symbol as usize]
        .as_ref()
        .expect("lexers only hold terminals with patterns")
}

fn can_start(grammar: &Grammar, symbol: Symbol, class: usize) -> bool {
    match pattern(grammar, symbol) {
        Pattern::Str(text) => class < NON_ASCII && text.as_bytes()[0] as usize == class,
        Pattern::Re(matcher) => matchers::can_start(*matcher, class),
    }
}

/// lark's `LineCounter`, plus the byte offset Rust slices by. `char_pos` counts code points, as Python
/// string indices do.
#[derive(Clone, Copy)]
struct LineCounter {
    byte_pos: usize,
    char_pos: usize,
    line: usize,
    column: usize,
    line_start_pos: usize,
}

impl LineCounter {
    fn feed(&mut self, value: &str, test_newline: bool) {
        if test_newline {
            let newlines = memchr_count(value);
            if newlines > 0 {
                self.line += newlines;
                let last = value.rfind('\n').expect("counted a newline");
                self.line_start_pos = self.char_pos + char_len(&value[..last]) + 1;
            }
        }
        self.byte_pos += value.len();
        self.char_pos += char_len(value);
        self.column = self.char_pos - self.line_start_pos + 1;
    }
}

fn memchr_count(value: &str) -> usize {
    value.bytes().filter(|&b| b == b'\n').count()
}

fn char_len(value: &str) -> usize {
    if value.is_ascii() {
        return value.len();
    }
    value.bytes().filter(|&b| (b & 0xC0) != 0x80).count()
}

/// The contextual lexer over one text: lark's `LexerState` and `ContextualLexer` together.
pub struct Lexer<'s> {
    language: &'static Language,
    text: &'s str,
    counter: LineCounter,
}

impl<'s> Lexer<'s> {
    pub fn new(language: &'static Language, text: &'s str) -> Lexer<'s> {
        Lexer {
            language,
            text,
            counter: LineCounter {
                byte_pos: 0,
                char_pos: 0,
                line: 1,
                column: 1,
                line_start_pos: 0,
            },
        }
    }

    /// The next token for parser state `state`, or None at the end of the text.
    pub fn lex(&mut self, state: u16) -> Result<Option<LexToken<'s>>, ParseError> {
        let grammar = self.language.grammar;
        let lexer = grammar.state_lexer[state as usize] as usize;
        let unmatched = match self.next_token(lexer) {
            Ok(token) => return Ok(token),
            Err(unmatched) => unmatched,
        };
        // The terminal may exist but not fit this state; asking the root lexer gives a better error.
        match self.next_token(grammar.root_lexer) {
            Ok(Some(token)) => Err(ParseError {
                line: token.span.line,
                column: token.span.column,
                kind: ErrorKind::UnexpectedToken {
                    token: grammar.symbols[token.kind as usize],
                    value: token.value.to_owned(),
                    expected: allowed(grammar, lexer),
                },
            }),
            _ => Err(unmatched),
        }
    }

    /// `BasicLexer.next_token`; Err holds the UnexpectedCharacters error.
    fn next_token(&mut self, lexer: usize) -> Result<Option<LexToken<'s>>, ParseError> {
        let grammar = self.language.grammar;
        let compiled = self.language.compiled();
        while self.counter.byte_pos < self.text.len() {
            let rest = &self.text[self.counter.byte_pos..];
            let Some((kind, len)) = scan(grammar, &compiled.candidates[lexer], rest) else {
                return Err(ParseError {
                    line: Some(self.counter.line),
                    column: Some(self.counter.column),
                    kind: ErrorKind::UnexpectedCharacters {
                        character: rest.chars().next().expect("not at the end"),
                    },
                });
            };
            let value = &rest[..len];
            let start = self.counter;
            self.counter.feed(value, compiled.newline[kind as usize]);
            if compiled.ignore[kind as usize] {
                continue;
            }
            let span = RawSpan {
                line: Some(start.line),
                column: Some(start.column),
                end_line: Some(self.counter.line),
                end_column: Some(self.counter.column),
                start_pos: Some(start.char_pos),
                end_pos: Some(self.counter.char_pos),
            };
            let kind = unless(grammar, lexer, kind, value);
            return Ok(Some(LexToken { kind, value, span }));
        }
        Ok(None)
    }
}

/// The first terminal, in lexer order, that matches at the start of `rest`, and its byte length.
fn scan(grammar: &Grammar, candidates: &[Vec<Symbol>], rest: &str) -> Option<(Symbol, usize)> {
    let first = rest.as_bytes()[0] as usize;
    let class = first.min(NON_ASCII);
    candidates[class]
        .iter()
        .find_map(|&symbol| match pattern(grammar, symbol) {
            Pattern::Str(text) => rest.starts_with(text).then_some((symbol, text.len())),
            Pattern::Re(matcher) => matchers::match_at(*matcher, rest).map(|len| (symbol, len)),
        })
}

/// `UnlessCallback`: a regex match that spells one of the strings it swallowed becomes that string's type.
fn unless(grammar: &Grammar, lexer: usize, kind: Symbol, value: &str) -> Symbol {
    let retypes = grammar.lexers[lexer].unless.iter();
    retypes
        .filter(|(regex, _)| *regex == kind)
        .flat_map(|(_, strings)| strings.iter())
        .find(|(text, _)| *text == value)
        .map_or(kind, |&(_, symbol)| symbol)
}

/// The scanner's `allowed_types - ignore_types`, which lark reports as the contextual lexer's expected set.
fn allowed(grammar: &Grammar, lexer: usize) -> Vec<&'static str> {
    let mut names: Vec<_> = grammar.lexers[lexer]
        .order
        .iter()
        .filter(|symbol| !grammar.ignore.contains(symbol))
        .map(|&symbol| grammar.symbols[symbol as usize])
        .collect();
    names.sort_unstable();
    names
}
