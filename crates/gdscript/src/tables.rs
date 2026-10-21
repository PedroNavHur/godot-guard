//! The shape of the generated parser tables: what `tools/gdscript/gen_tables.py` reads out of the Lark
//! objects gdtoolkit builds. Symbols are indices into `Grammar::symbols`; terminals come first.

pub type Symbol = u16;

pub struct Grammar {
    pub symbols: &'static [&'static str],
    /// Symbols below this index are terminals, including `$END` and the indenter's `_INDENT`/`_DEDENT`.
    pub terminal_count: usize,
    /// Per terminal, its pattern; None for terminals the lexer never produces.
    pub patterns: &'static [Option<Pattern>],
    /// Terminals lark counts newlines in (`_regexp_has_newline`).
    pub newline_types: &'static [Symbol],
    pub ignore: &'static [Symbol],
    /// The contextual lexer's distinct state lexers, then the root lexer.
    pub lexers: &'static [Lexer],
    pub root_lexer: usize,
    /// Per parser state, its index into `lexers`.
    pub state_lexer: &'static [u16],
    /// Per parser state, its actions sorted by symbol; see `Action::decode`.
    pub states: &'static [&'static [u32]],
    pub rules: &'static [Rule],
    pub start_state: u16,
    pub end_state: u16,
}

pub enum Pattern {
    Str(&'static str),
    Re(Matcher),
}

/// The hand-written matcher for one of the grammar's regex terminals; see `matchers.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Matcher {
    WsInline,
    Hex,
    Bin,
    TypeHint,
    Number,
    LongString,
    LongRString,
    RegularString,
    RegularRString,
    Newline,
    Comment,
    LineCont,
    Name,
    Anything,
}

/// One BasicLexer: its terminals in lark's match order, and `_create_unless`'s retypings.
pub struct Lexer {
    pub order: &'static [Symbol],
    /// (regex terminal, [(exact value, terminal it becomes)]).
    pub unless: &'static [(Symbol, &'static [(&'static str, Symbol)])],
}

/// One grammar rule and the tree-building chain lark wraps around it.
pub struct Rule {
    pub origin: Symbol,
    pub size: usize,
    /// The tree's name: the alias, else the rule's own name.
    pub data: &'static str,
    /// `?rule` without an alias: a single child replaces the tree.
    pub expand1: bool,
    /// ChildFilterLALR_NoPlaceholders: (child index, inline that `_rule` child's children). None keeps all.
    pub include: Option<&'static [(u16, bool)]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Shift a terminal, or go to a state after a reduce.
    Shift(u16),
    Reduce(u16),
}

const REDUCE_BIT: u16 = 0x8000;
const NO_ACTION: u16 = u16::MAX;

impl Action {
    fn decode(word: u16) -> Option<Action> {
        match word {
            NO_ACTION => None,
            _ if word & REDUCE_BIT != 0 => Some(Action::Reduce(word & !REDUCE_BIT)),
            _ => Some(Action::Shift(word)),
        }
    }
}

/// The parse table spread out, one word per (state, symbol), so a lookup is one load.
pub struct DenseTable {
    words: Vec<u16>,
    symbols: usize,
}

impl DenseTable {
    pub fn new(grammar: &Grammar) -> DenseTable {
        let symbols = grammar.symbols.len();
        let mut words = vec![NO_ACTION; grammar.states.len() * symbols];
        for (state, actions) in grammar.states.iter().enumerate() {
            for &entry in actions.iter() {
                words[state * symbols + (entry >> 16) as usize] = entry as u16;
            }
        }
        DenseTable { words, symbols }
    }

    pub fn action(&self, state: u16, symbol: Symbol) -> Option<Action> {
        Action::decode(self.words[state as usize * self.symbols + symbol as usize])
    }
}

impl Grammar {
    /// The terminals `state` has an action for, as lark lists them in UnexpectedToken's `expected`.
    pub fn expected(&self, state: u16) -> Vec<&'static str> {
        let mut names: Vec<_> = self.states[state as usize]
            .iter()
            .map(|entry| (entry >> 16) as usize)
            .filter(|&symbol| symbol < self.terminal_count)
            .map(|symbol| self.symbols[symbol])
            .collect();
        names.sort_unstable();
        names
    }

    pub fn symbol(&self, name: &str) -> Option<Symbol> {
        self.symbols[..self.terminal_count]
            .iter()
            .position(|&s| s == name)
            .map(|index| index as Symbol)
    }
}
