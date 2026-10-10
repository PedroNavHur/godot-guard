"""Parity with pedro-best-practices' scorers on real repositories.

Set GODOT_GUARD_PARITY_REPOS to repository paths separated by the OS path separator.
GODOT_GUARD_PBP_SCRIPTS overrides where the reference scripts live.
"""

from __future__ import annotations

import importlib
import os
import sys
import unittest
from pathlib import Path

from godot_guard import complexity
from godot_guard.config import walk_scripts
from godot_guard.rules import Surfaces, parse_tree

REPOS = [Path(p) for p in os.environ.get("GODOT_GUARD_PARITY_REPOS", "").split(os.pathsep) if p]
SCRIPTS = Path(os.environ.get("GODOT_GUARD_PBP_SCRIPTS", "~/.agents/skills/pedro-best-practices/scripts")).expanduser()


def reference(name: str):
    if str(SCRIPTS) not in sys.path:
        sys.path.insert(0, str(SCRIPTS))
    return importlib.import_module(name)


def scores(functions) -> list[tuple]:
    return [(f.line, f.end_line, f.cognitive, f.cyclomatic) for f in functions]


@unittest.skipUnless(REPOS, "set GODOT_GUARD_PARITY_REPOS to run the parity tests")
@unittest.skipUnless((SCRIPTS / "complexity_gdscript.py").is_file(), f"reference scripts not found in {SCRIPTS}")
class ParityTest(unittest.TestCase):
    def test_complexity_scores_match(self):
        gdscript = reference("complexity_gdscript")
        mismatches, functions = [], 0
        for root in REPOS:
            for relative in walk_scripts(root, root):
                path = root / relative
                expected = scores(gdscript.analyze(str(path)))
                actual = scores(complexity.score(parse_tree(path.read_text(encoding="utf-8"))))
                functions += len(expected)
                if expected != actual:
                    mismatches.append(f"{root.name}/{relative}: {expected} != {actual}")
        print(f"\ncomplexity parity: {functions} functions, {len(mismatches)} mismatches", file=sys.stderr)
        self.assertEqual(mismatches, [])

    def test_interface_totals_match(self):
        mismatches, scripts = [], 0
        for root in REPOS:
            expected, actual = reference_totals(root), totals(root)
            scripts += len(expected)
            mismatches += [f"{root.name}/{p}: {expected[p]} != {actual.get(p)}" for p in expected if expected[p] != actual.get(p)]
        print(f"\ninterface parity: {scripts} scripts, {len(mismatches)} mismatches", file=sys.stderr)
        self.assertEqual(mismatches, [])


def reference_totals(root: Path) -> dict[str, int | None]:
    """godot_interface.py's total per script, None for a Resource script."""
    godot_interface = reference("godot_interface")
    by_path = {r: godot_interface.parse(r, (root / r).read_text(encoding="utf-8")) for r in walk_scripts(root, root)}
    by_class = {s.class_name: s for s in by_path.values() if s.class_name}
    resources = {r for r, s in by_path.items() if godot_interface.is_resource(s, by_class, by_path)}
    return {r: None if r in resources else s.total for r, s in by_path.items()}


def totals(root: Path) -> dict[str, int | None]:
    surfaces = Surfaces(root)
    found = {}
    for relative in walk_scripts(root, root):
        surface = surfaces.surface(relative, (root / relative).read_text(encoding="utf-8"))
        found[relative] = None if surface is None else surface.total
    return found

if __name__ == "__main__":
    unittest.main()
