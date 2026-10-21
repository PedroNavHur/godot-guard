"""Emit gdformat test cases from gdtoolkit 4.5.0, the oracle the Rust `gdformat` crate must reproduce.

Usage: python gdformat_cases.py [--formatted-trees] [--jobs N] [--variants A,B...] FILE...
One JSON object per line, per case. A case is a file's source, or a perturbed variant of it, with options:
  {"id", "path", "variant", "line_length", "spaces", "code", "tree", "comments",
   "expected": str | null, "error": str | null, "safety": "ok" | violation name | null,
   "formatted": {"tree", "comments"} | null}
`code` is the source as gdformat reads it (text mode: universal newlines). `tree` and `comments` are the
oracle's parse trees of `code`, in dump_tree.py's encoding; null when `error` names the exception that
parsing or formatting raised. `expected` is format_code's output. `safety` is check_formatting_safety's
verdict, run as gdformat runs it, only when `expected` differs from `code`. `formatted` holds the trees of
`expected`, so a port can run the safety checks without a parser, with --formatted-trees.

Variants (each from the file's source; `unfolded` from `narrow`'s output):
  original     line length 100, tabs
  narrow       line length 40: folds most expressions
  unfolded     narrow's output at line length 100: joins folded expressions back
  spaces4      output indented with 4 spaces
  indent2      input indented with 2 spaces per tab, output tabs
  loose        every inline space run widened, a space added around punctuation
  tight        inline spaces next to punctuation removed
  blank3       every blank line tripled
  noblank      every blank line removed
  trailing     trailing spaces and tabs on every line
"""

import json
import multiprocessing
import re
import sys

from gdtoolkit.formatter import check_formatting_safety, format_code
from gdtoolkit.parser import parser

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from dump_tree import node  # noqa: E402  pylint: disable=wrong-import-position

PUNCTUATION = re.compile(r"[^\w\"']")
FORMATTED_TREES = False
VARIANTS = None


def lexed(code):
    """The source's tokens, whitespace included, or None when lark's basic lexer can't split it."""
    try:
        return [t for t in parser._parser_with_metadata.lex(code + "\n", dont_ignore=True)
                if t.type not in ("_INDENT", "_DEDENT")]
    except Exception:  # pylint: disable=broad-except
        return None


def respace(code, widen):
    """Rewrite the inline whitespace between tokens; leading indentation, strings and comments stay."""
    tokens = lexed(code)
    if tokens is None:
        return None
    out, previous, cursor = [], None, 0
    for index, token in enumerate(tokens):
        out.append(code[cursor:token.start_pos])
        text = code[token.start_pos:token.end_pos]
        if token.type == "WS_INLINE":
            following = tokens[index + 1] if index + 1 < len(tokens) else None
            out.append("  " if widen else "" if _tightenable(previous, following) else text)
        elif widen and _joint(previous, token):
            out.append(" " + text)
        else:
            out.append(text)
        previous = token
        cursor = token.end_pos
    return "".join(out) + code[cursor:]


def _joint(previous, token):
    """Two tokens touching on one line, one of them punctuation."""
    if previous is None or "_NL" in (previous.type, token.type) or previous.type == "WS_INLINE":
        return False
    return bool(PUNCTUATION.match(str(token)[:1]) or PUNCTUATION.match(str(previous)[-1:]))


def _tightenable(previous, following):
    if previous is None or following is None or "_NL" in (previous.type, following.type):
        return False
    return bool(PUNCTUATION.match(str(previous)[-1:]) or PUNCTUATION.match(str(following)[:1]))


def variants(code):
    yield "original", code, 100, None
    yield "narrow", code, 40, None
    yield "spaces4", code, 100, 4
    yield "indent2", re.sub(r"(?m)^\t+", lambda m: "  " * len(m.group(0)), code), 100, None
    yield "loose", respace(code, widen=True), 100, None
    yield "tight", respace(code, widen=False), 100, None
    yield "blank3", re.sub(r"(?m)^[ \t]*$", "\n\n", code), 100, None
    yield "noblank", re.sub(r"\n[ \t]*(?=\n)", "", code), 100, None
    yield "trailing", re.sub(r"(?m)$", " \t ", code), 100, None


def case(path, variant, code, line_length, spaces):
    record = {"id": f"{path}#{variant}", "path": path, "variant": variant, "line_length": line_length,
              "spaces": spaces, "code": code, "tree": None, "comments": None, "expected": None,
              "error": None, "safety": None, "formatted": None}
    try:
        tree = parser.parse(code, gather_metadata=True)
        comments = parser.parse_comments(code)
        record["tree"], record["comments"] = node(tree), node(comments)
        expected = format_code(code, line_length, spaces_for_indent=spaces,
                               parse_tree=tree, comment_parse_tree=comments)
    except Exception as exc:  # pylint: disable=broad-except
        record["error"] = type(exc).__name__
        return record
    record["expected"] = expected
    if expected != code:
        try:
            check_formatting_safety(code, expected, line_length, given_code_parse_tree=tree,
                                    given_code_comment_parse_tree=comments, spaces_for_indent=spaces)
            record["safety"] = "ok"
        except Exception as exc:  # pylint: disable=broad-except
            record["safety"] = type(exc).__name__
        if FORMATTED_TREES:
            try:
                record["formatted"] = {"tree": node(parser.parse(expected, gather_metadata=True)),
                                       "comments": node(parser.parse_comments(expected))}
            except Exception:  # pylint: disable=broad-except
                pass
    return record


def cases(path):
    with open(path, encoding="utf-8") as handle:
        code = handle.read()
    lines = []
    narrow = None
    for variant, variant_code, line_length, spaces in variants(code):
        if variant_code is None or (VARIANTS is not None and variant not in VARIANTS):
            continue
        record = case(path, variant, variant_code, line_length, spaces)
        lines.append(json.dumps(record, ensure_ascii=False, separators=(",", ":")))
        if variant == "narrow":
            narrow = record["expected"]
    if narrow is not None and (VARIANTS is None or "unfolded" in VARIANTS):
        record = case(path, "unfolded", narrow, 100, None)
        lines.append(json.dumps(record, ensure_ascii=False, separators=(",", ":")))
    return lines


def _init(formatted_trees, variants_wanted):
    global FORMATTED_TREES, VARIANTS  # pylint: disable=global-statement
    FORMATTED_TREES, VARIANTS = formatted_trees, variants_wanted


def main():
    args = sys.argv[1:]
    formatted_trees = "--formatted-trees" in args
    args = [a for a in args if a != "--formatted-trees"]
    jobs, variants_wanted = 4, None
    while args[:1] in (["--jobs"], ["--variants"]):
        if args[0] == "--jobs":
            jobs = int(args[1])
        else:
            variants_wanted = set(args[1].split(","))
        args = args[2:]
    sys.setrecursionlimit(100000)
    with multiprocessing.Pool(jobs, initializer=_init, initargs=(formatted_trees, variants_wanted)) as pool:
        for lines in pool.imap(cases, args, chunksize=4):
            for line in lines:
                print(line)


if __name__ == "__main__":
    main()
