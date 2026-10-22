"""Dump gdtoolkit 4.5.0's parse trees as JSON, the oracle the Rust `gdscript` crate must reproduce.

Usage: python dump_tree.py FILE...  One JSON object per line, per file:
  {"path": P, "tree": NODE, "comments": NODE}           parsed
  {"path": P, "error": {"line", "column", "type", "message"}}   rejected
A tree is {"t": rule, "c": [NODE...], "m": SPAN|null}; a token is {"k": type, "v": value, "p": SPAN}.
SPAN is [line, column, end_line, end_column, start_pos, end_pos], as lark reports them: 1-based lines and
columns, positions in code points. `tree` is parse(source, gather_metadata=True); `comments` is parse_comments.
"""

import json
import sys

from gdtoolkit.parser import parser
from lark import Token, Tree

FIELDS = ("line", "column", "end_line", "end_column", "start_pos", "end_pos")


def span(obj):
    if isinstance(obj, Tree) and obj.meta.empty:
        return None
    holder = obj if isinstance(obj, Token) else obj.meta
    return [getattr(holder, name, None) for name in FIELDS]


def node(obj):
    if isinstance(obj, Token):
        return {"k": obj.type, "v": str(obj), "p": span(obj)}
    return {"t": str(obj.data), "c": [node(child) for child in obj.children], "m": span(obj)}


def error(exc):
    text = str(exc)
    first = text.splitlines()[0] if text else type(exc).__name__
    return {"line": getattr(exc, "line", None), "column": getattr(exc, "column", None),
            "type": type(exc).__name__, "message": first}


def dump(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        source = handle.read()
    try:
        return {"path": path, "tree": node(parser.parse(source, gather_metadata=True)),
                "comments": node(parser.parse_comments(source))}
    except Exception as exc:  # lark raises many parse error types
        return {"path": path, "error": error(exc)}


def main():
    for path in sys.argv[1:]:
        print(json.dumps(dump(path), ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
