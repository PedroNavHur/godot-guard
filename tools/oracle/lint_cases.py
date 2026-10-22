"""Write the GDScript snippets in gdtoolkit's linter tests as .gd files, for the gdlint parity harness.

Usage: python -I lint_cases.py TESTS_DIR OUT_DIR
Reads every test_*.py in TESTS_DIR with the `ast` module, without importing or running it, and writes each
string literal that is an item of a list or tuple, or the first argument of a call (the shapes the tests pass
code in), to OUT_DIR/<test module>_<n>.gd. Strings that are not GDScript are harmless: the harness drops the
files the oracle parser rejects. Prints the number of files written.
"""

import ast
import pathlib
import sys


def candidates(node):
    if isinstance(node, (ast.List, ast.Tuple)):
        return node.elts
    if isinstance(node, ast.Call):
        return node.args[:1]
    return []


def snippets(tree):
    for node in ast.walk(tree):
        yield from (
            candidate.value
            for candidate in candidates(node)
            if isinstance(candidate, ast.Constant) and isinstance(candidate.value, str)
        )


def main():
    tests_dir, out_dir = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    out_dir.mkdir(parents=True, exist_ok=True)
    written = 0
    for test_file in sorted(tests_dir.glob("test_*.py")):
        seen = set()
        tree = ast.parse(test_file.read_text(encoding="utf-8"), filename=str(test_file))
        for code in snippets(tree):
            if code in seen:
                continue
            seen.add(code)
            path = out_dir / "{}_{:03d}.gd".format(test_file.stem, len(seen))
            path.write_bytes(code.encode("utf-8"))
            written += 1
    print(written)


if __name__ == "__main__":
    main()
