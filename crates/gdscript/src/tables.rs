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
