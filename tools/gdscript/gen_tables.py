"""Generate the Rust `gdscript` crate's parser tables from gdtoolkit 4.5.0's own lark parsers.

Usage: python gen_tables.py OUT_DIR   (run with the oracle venv's python)

Writes OUT_DIR/gdscript.rs and OUT_DIR/comments.rs. Everything is read from the Lark objects gdtoolkit
builds, not recomputed: the LALR table, each contextual-lexer state's terminal order after lark's sort
and `_create_unless`, the unless retypings, and each rule's tree-building chain (ExpandSingleChild,
ChildFilterLALR_NoPlaceholders, the tree name). The Rust side ports only lark's runtime.

Regex terminals are matched by hand-written Rust matchers. MATCHERS maps each regex source lark compiles
to the matcher that implements it; an unknown source stops generation, so a grammar change cannot slip
through with a stale matcher.
"""

import sys
from importlib.metadata import version
from pathlib import Path

import lark
from gdtoolkit.parser import parser as gd_parser
from gdtoolkit.parser.gdscript_indenter import GDScriptIndenter
from lark.lexer import PatternRE, PatternStr
from lark.parse_tree_builder import ChildFilterLALR_NoPlaceholders, ExpandSingleChild, PropagatePositions
from lark.parsers.lalr_analysis import Shift

_NAME_START = "[\\p{XID_Start}_\u037a-\u037d\u309b-\u309f\ufe70-\ufe74\ufe76-\ufefc\x00]"
_NAME_CONT = "[\\p{XID_Continue}_\u037a-\u037d\ufe76-\ufefc\x00]*"
_NAME = _NAME_START + _NAME_CONT
_TYPE = _NAME + "(?:(?:\\ )?\\.(?:\\ )?" + _NAME + ")*"
_INT = "[0-9](?:(?:[0-9]|_))*"
_SIGN = "(?:(?:\\+|\\-))?"
_EXP = "(?:e|E)" + _SIGN + _INT

MATCHERS = {
    "(?:(?:\\ |\t))+": "WsInline",
    "-?0x[a-fA-F0-9][a-fA-F0-9_]*": "Hex",
    "-?0b[01][01_]*": "Bin",
    _TYPE + "(?:\\[" + _TYPE + "(?:(?:\\ )?,(?:\\ )?" + _TYPE + ")?\\])?": "TypeHint",
    _SIGN + "(?:(?:" + _INT + _EXP + "|(?:" + _INT + "\\.(?:" + _INT + ")?|\\." + _INT + ")(?:" + _EXP + ")?)|"
    + _INT + ")": "Number",
    "(?:(?i:(?s:\"\"\".*?(?<!\\\\)(\\\\\\\\)*?\"\"\"))|(?i:(?s:'''.*?(?<!\\\\)(\\\\\\\\)*?''')))": "LongString",
    "(?:(?i:(?s:r\"\"\".*?(?<!\\\\)(\\\\\\\\)*?\"\"\"))|(?i:(?s:r'''.*?(?<!\\\\)(\\\\\\\\)*?''')))": "LongRString",
    "(?i:(\"(?!\"\").*?(?<!\\\\)(\\\\\\\\)*?\"|'(?!'').*?(?<!\\\\)(\\\\\\\\)*?'))": "RegularString",
    "(?i:(r\"(?!\"\").*?(?<!\\\\)(\\\\\\\\)*?\"|r'(?!'').*?(?<!\\\\)(\\\\\\\\)*?'))": "RegularRString",
    "(?:(?:\r?\n[\t ]*|#[^\n]*))+": "Newline",
    "#[^\n]*": "Comment",
    "\\\\[\t \x0c]*\r?\n": "LineCont",
    _NAME: "Name",
    "[^#\"']+": "Anything",
}


def rust_str(text):
    out = []
    for char in text:
        code = ord(char)
        if char in '"\\':
            out.append("\\" + char)
        elif 0x20 <= code < 0x7F:
            out.append(char)
        else:
            out.append("\\u{%x}" % code)
    return '"' + "".join(out) + '"'


def chain_of(callback):
    """Unwrap lark's callback chain into (expand1, to_include or None, tree name)."""
    assert type(callback) is PropagatePositions, callback
    builder = callback.node_builder
    to_include = None
    if type(builder) is ChildFilterLALR_NoPlaceholders:
        to_include = builder.to_include
        builder = builder.node_builder
    expand1 = isinstance(builder, ExpandSingleChild)
    if expand1:
        builder = builder.node_builder
    assert builder.func is lark.Tree and len(builder.args) == 1, builder
    return expand1, to_include, builder.args[0]


class Emitter:
    def __init__(self, lark_parser):
        frontend = lark_parser.parser
        self.lalr = frontend.parser.parser
        self.table = self.lalr.parse_table
        self.contextual = frontend.lexer.lexer if hasattr(frontend.lexer, "lexer") else frontend.lexer
        self.terminals = {t.name: t for t in lark_parser.terminals}
        self.ignore = list(lark_parser.lexer_conf.ignore)
        self.rules = list(self.lalr.callbacks.keys())
        self.rule_index = {id(rule): i for i, rule in enumerate(self.rules)}
        self.symbols = self._symbols()
        self.symbol_index = {name: i for i, name in enumerate(self.symbols)}

    def _symbols(self):
        nonterminals = {rule.origin.name for rule in self.rules}
        terminals = set(self.terminals)
        for actions in self.table.states.values():
            terminals.update(name for name in actions if name not in nonterminals)
        # Terminals first, so `id < terminal_count` tells them apart; sorted for a stable diff.
        return sorted(terminals) + sorted(nonterminals)

    def lexers(self):
        """Distinct state lexers in first-seen order, then the root lexer."""
        found, by_id, state_lexer = [], {}, []
        for state in range(len(self.table.states)):
            lexer = self.contextual.lexers[state]
            if id(lexer) not in by_id:
                by_id[id(lexer)] = len(found)
                found.append(lexer)
            state_lexer.append(by_id[id(lexer)])
        found.append(self.contextual.root_lexer)
        return found, state_lexer

    def emit_lexer(self, lexer):
        scanner = lexer.scanner
        order = ", ".join(str(self.symbol_index[t.name]) for t in scanner.terminals)
        unless = []
        for name, callback in sorted(lexer.callback.items()):
            assert type(callback).__name__ == "UnlessCallback", (name, callback)
            pairs = ", ".join(
                "(%s, %d)" % (rust_str(t.pattern.value), self.symbol_index[t.name]) for t in callback.scanner.terminals
            )
            unless.append("(%d, &[%s])" % (self.symbol_index[name], pairs))
        return "    Lexer { order: &[%s], unless: &[%s] },\n" % (order, ", ".join(unless))

    def emit_terminal(self, name):
        terminal = self.terminals.get(name)
        if terminal is None:
            return "    None,\n"
        pattern = terminal.pattern
        if isinstance(pattern, PatternStr):
            assert not pattern.flags, (name, pattern.flags)
            return "    Some(Pattern::Str(%s)),\n" % rust_str(pattern.value)
        assert isinstance(pattern, PatternRE)
        source = pattern.to_regexp()
        if source not in MATCHERS:
            raise SystemExit("no Rust matcher for %s: %r" % (name, source))
        return "    Some(Pattern::Re(Matcher::%s)),\n" % MATCHERS[source]

    def emit_rule(self, rule):
        expand1, to_include, data = chain_of(self.lalr.callbacks[rule])
        if to_include is None:
            include = "None"
        else:
            include = "Some(&[%s])" % ", ".join("(%d, %s)" % (i, str(x).lower()) for i, x in to_include)
        return "    Rule { origin: %d, size: %d, data: %s, expand1: %s, include: %s },\n" % (
            self.symbol_index[rule.origin.name], len(rule.expansion), rust_str(str(data)),
            str(expand1).lower(), include)

    def emit_state(self, actions):
        encoded = []
        for name, (action, arg) in sorted(actions.items(), key=lambda item: self.symbol_index[item[0]]):
            if action is Shift:
                value = arg
            else:
                value = 0x8000 | self.rule_index[id(arg)]
            encoded.append("%d" % ((self.symbol_index[name] << 16) | value))
        return "    &[%s],\n" % ", ".join(encoded)

    def render(self, title):
        newline_types = self.contextual.root_lexer.newline_types
        lexers, state_lexer = self.lexers()
        terminal_count = len(self.symbols) - len({r.origin.name for r in self.rules})
        (start_state,) = self.table.start_states.values()
        (end_state,) = self.table.end_states.values()
        out = [
            "// @generated by tools/gdscript/gen_tables.py from gdtoolkit %s (lark %s): %s. Do not edit.\n\n"
            % (version("gdtoolkit"), lark.__version__, title),
            "use crate::tables::{Grammar, Lexer, Matcher, Pattern, Rule};\n\n",
            "pub static GRAMMAR: Grammar = Grammar {\n",
            "    symbols: &SYMBOLS,\n    terminal_count: %d,\n    patterns: &PATTERNS,\n" % terminal_count,
            "    newline_types: &[%s],\n" % ", ".join(str(self.symbol_index[n]) for n in sorted(newline_types)),
            "    ignore: &[%s],\n" % ", ".join(str(self.symbol_index[n]) for n in self.ignore),
            "    lexers: &LEXERS,\n    root_lexer: %d,\n    state_lexer: &STATE_LEXER,\n" % (len(lexers) - 1),
            "    states: &STATES,\n    rules: &RULES,\n",
            "    start_state: %d,\n    end_state: %d,\n};\n\n" % (start_state, end_state),
            "static SYMBOLS: [&str; %d] = [\n" % len(self.symbols),
            "".join("    %s,\n" % rust_str(name) for name in self.symbols), "];\n\n",
            "static PATTERNS: [Option<Pattern>; %d] = [\n" % terminal_count,
            "".join(self.emit_terminal(name) for name in self.symbols[:terminal_count]), "];\n\n",
            "static LEXERS: [Lexer; %d] = [\n" % len(lexers),
            "".join(self.emit_lexer(lexer) for lexer in lexers), "];\n\n",
            "static STATE_LEXER: [u16; %d] = [%s];\n\n" % (len(state_lexer), ", ".join(map(str, state_lexer))),
            "static RULES: [Rule; %d] = [\n" % len(self.rules),
            "".join(self.emit_rule(rule) for rule in self.rules), "];\n\n",
            "// Per state, sorted by symbol: (symbol << 16) | state for a shift or goto, | 0x8000 | rule for a reduce.\n",
            "static STATES: [&[u32]; %d] = [\n" % len(self.table.states),
            "".join(self.emit_state(self.table.states[s]) for s in range(len(self.table.states))), "];\n",
        ]
        return "".join(out)


def check_assumptions(lark_parser, emitter):
    """Fail loudly where the Rust runtime assumes more than lark guarantees."""
    options = lark_parser.options
    assert options.parser == "lalr" and options.lexer == "contextual", (options.parser, options.lexer)
    assert options.propagate_positions is True and options.maybe_placeholders is False
    # The Rust driver always runs the GDScript indenter, as gdtoolkit does for both grammars.
    assert type(options.postlex) is GDScriptIndenter, options.postlex
    states = emitter.table.states
    assert sorted(states) == list(range(len(states))), "parser states are not 0..n"
    # A dense action word is a state, 0x8000 | rule, or 0xFFFF for none.
    assert len(states) < 0x8000 and len(emitter.rules) < 0x7FFF
    assert len(emitter.symbols) < 0x10000
    for terminal in emitter.terminals.values():
        assert terminal.pattern.value, terminal.name


def main():
    out_dir = Path(sys.argv[1])
    out_dir.mkdir(parents=True, exist_ok=True)
    for name, lark_parser, title in [
        ("gdscript.rs", gd_parser._parser_with_metadata, "gdscript.lark with propagate_positions"),
        ("comments.rs", gd_parser._comment_parser, "comments.lark"),
    ]:
        emitter = Emitter(lark_parser)
        check_assumptions(lark_parser, emitter)
        (out_dir / name).write_text(emitter.render(title), encoding="utf-8")


if __name__ == "__main__":
    main()
