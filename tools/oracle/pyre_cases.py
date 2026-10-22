"""Write what Python's `re.fullmatch` says for each name-check pattern and name, the oracle `gdlint::pyre` meets.

Usage: python -I pyre_cases.py > crates/gdlint/tests/fixtures/pyre_cases.json
Patterns are gdlint 4.5.0's defaults, hexstead's override, and constructs whose meaning differs between Python
`re` and the Rust `regex` crate. A pattern Python refuses is recorded with "error": true; the port must refuse
it too. A pattern Python accepts must give the same answer for every name; "matches" holds one 1 or 0 per name, in
the order of "names".
"""

import json
import re
import sys
import warnings

from gdtoolkit.linter import DEFAULT_CONFIG

HEXSTEAD = ["_?[a-z][a-z0-9]*(_[a-z0-9]+)*"]

TRICKY = [
    "[a&&b]", "[+--]", "[a~~b]", "[[:alpha:]]", r"\<a\>", "a{", "a{1,x}", "a{,2}", "a{2,}", "a{}", "a{,}",
    "#", " ", "a|ab", "(?i)[a-z]+", "(?i:A)b", "x(?#comment)y", r"\w+\Z", r"\d+", r"\s", r"[\w-]+", r"[\]a]",
    r"[]a]", "[^]a]", r"[\b]", r"\x41", r"é", "é+", "a*?", "a+?b", "(?P<n>a)b", r"[\d-z]", "[a-]", "[-a]",
    "^a$", "a$", r"\Aa", r"\bA", "(a)\\1", "(?=a)a", "(?<=a)b", "(?x)a b", "(?a)\\w", "a**", "a*+", "a(?i)",
    "a{3,1}", r"\q", "[a", "[a--b]", "(", "*a", r"\0", "a|", "(?s).", ".", "[^a-z]+",
]

NAMES = [
    "", "a", "ab", "abc", "abC", "A", "Ab", "AB", "_a", "_A", "a_b", "a__b", "A_B", "_on_Button_pressed",
    "_on_button_pressed", "snake_case", "PascalCase", "UPPER_SNAKE", "x1", "1x", "&", "+", ",", "-", "~", ":",
    "<a>", "a{", "a{1,x}", "aa", "aaa", "a{}", "#", " ", "xy", "é", "éé", "١", "²", "_", "]", "]a",
    "\b", "aab", "ab_", "a-b", "K", "k", "K", "İ", "i", "a\u0301", "a\u203fb", "\u2160", "\x1f",
]


def case(pattern):
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        try:
            compiled = re.compile(pattern)
        except re.error as error:
            return {"pattern": pattern, "error": True, "reason": str(error)}
    return {"pattern": pattern, "error": False,
            "matches": "".join("1" if compiled.fullmatch(name) else "0" for name in NAMES)}


def main():
    defaults = [value for value in DEFAULT_CONFIG.values() if isinstance(value, str)]
    patterns = list(dict.fromkeys(defaults + HEXSTEAD + TRICKY))
    lines = [json.dumps(case(pattern), ensure_ascii=False) for pattern in patterns]
    sys.stdout.write('{"names": %s,\n"cases": [\n%s\n]}\n' % (json.dumps(NAMES), ",\n".join(lines)))


if __name__ == "__main__":
    main()
