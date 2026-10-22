"""Dump godot-guard 0.4.0's tree-rule results with the tree they ran on, the oracle for the Rust `gdrules` crate.

Usage: python dump_rules.py FILE...  One JSON object per line, per file:
  {"path": P, "tree": NODE, "rules": RULES}      parsed
  {"path": P, "error": {...}}                    rejected, as dump_tree.py reports it
NODE is dump_tree.py's encoding of parse(source, gather_metadata=True). RULES is
  {"functions": [[name, line, end_line, cognitive|null, cyclomatic]...],   complexity.score
   "nested_ternaries": [line...],                                          complexity.nested_ternaries
   "patterns": [[rule, [[line, message]...]]...]}                          patterns.scan, in its key order
A file the rules raise on gets {"path", "tree", "crash": "Type: message"} instead of "rules".
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from dump_tree import error, node  # noqa: E402
from gdtoolkit.parser import parser  # noqa: E402
from godot_guard import complexity, patterns  # noqa: E402


def rules(tree):
    return {
        "functions": [[f.name, f.line, f.end_line, f.cognitive, f.cyclomatic] for f in complexity.score(tree)],
        "nested_ternaries": complexity.nested_ternaries(tree),
        "patterns": [[rule, [list(hit) for hit in hits]] for rule, hits in patterns.scan(tree).items()],
    }


def dump(path):
    with open(path, encoding="utf-8", errors="replace") as handle:
        source = handle.read()
    try:
        tree = parser.parse(source, gather_metadata=True)
    except Exception as exc:  # lark raises many parse error types
        return {"path": path, "error": error(exc)}
    result = {"path": path, "tree": node(tree)}
    try:
        result["rules"] = rules(tree)
    except Exception as exc:  # the oracle's own failure is a result the port must match
        result["crash"] = f"{type(exc).__name__}: {exc}"
    return result


def main():
    for path in sys.argv[1:]:
        print(json.dumps(dump(path), ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
