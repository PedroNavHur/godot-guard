from __future__ import annotations

import json
import unittest

from godot_guard import baseline
from godot_guard.check import check, update_baseline
from godot_guard.rules import Surfaces, analyze

from .helpers import Repo, captured

CONFIG = "limits:\n  cognitive: 2\n  cyclomatic: 16\nrules:\n  nested-ternary: off\n"


def branches(name: str, count: int) -> str:
    """A function with `count` sequential ifs: cognitive `count`."""
    body = "".join(f"\tif x == {n}:\n\t\treturn {n}\n" for n in range(count))
    return f"func {name}(x: int) -> int:\n{body}\treturn -1\n"


class BaselineTest(unittest.TestCase):
    def setUp(self):
        source = "extends Node\n\n\n" + branches("busy", 4) + "\n\n" + branches("calm", 1)
        self.repo = Repo({"a.gd": source, "b.gd": "extends Node\n\n\n" + branches("other", 3)}, CONFIG)
        self.addCleanup(self.repo.cleanup)
        self.config = self.repo.config()

    def record(self, entries: dict) -> None:
        data = baseline.empty()
        data["cognitive-complexity"] = entries
        baseline.write(self.config.baseline_path, data)

    def recorded(self) -> dict:
        return json.loads(self.config.baseline_path.read_text())["cognitive-complexity"]

    def test_new_violation_is_an_error(self):
        status, output = captured(check, self.config)
        self.assertEqual(status, 1)
        self.assertIn("a.gd:4: busy has cognitive complexity 4 (max 2) (cognitive-complexity)", output)
        self.assertIn("2 errors, 0 warnings, 2 files", output)

    def test_baselined_at_its_value_passes(self):
        self.record({"a.gd::busy": 4, "b.gd::other": 3})
        self.assertEqual(captured(check, self.config)[0], 0)

    def test_worsened_is_an_error(self):
        self.record({"a.gd::busy": 3, "b.gd::other": 3})
        status, output = captured(check, self.config)
        self.assertEqual(status, 1)
        self.assertIn("busy has cognitive complexity 4 (baseline 3, max 2)", output)

    def test_loose_entry_is_an_error(self):
        self.record({"a.gd::busy": 6, "b.gd::other": 3})
        status, output = captured(check, self.config)
        self.assertEqual(status, 1)
        self.assertIn("below its baseline 6: lower it with `godot-guard baseline`", output)

    def test_stale_entry_within_limit_is_an_error(self):
        self.record({"a.gd::busy": 4, "a.gd::calm": 3, "b.gd::other": 3})
        status, output = captured(check, self.config)
        self.assertEqual(status, 1)
        self.assertIn("calm has cognitive complexity 1 is within the limit 2", output)

    def test_missing_function_points_at_the_baseline(self):
        self.record({"a.gd::busy": 4, "a.gd::gone": 5, "b.gd::other": 3})
        status, output = captured(check, self.config)
        self.assertEqual(status, 1)
        self.assertIn(".godot-guard-baseline.json:4: baseline entry 'a.gd::gone' no longer exists", output)

    def test_subset_only_judges_its_own_entries(self):
        self.record({"a.gd::busy": 4, "b.gd::gone": 5, "b.gd::other": 3})
        status, output = captured(check, self.config, [str(self.repo.root / "a.gd")])
        self.assertEqual(status, 0, output)
        self.assertIn("1 file", output)

    def test_baseline_tightens_and_never_adds(self):
        self.record({"a.gd::busy": 6, "a.gd::calm": 3, "a.gd::gone": 5})
        status, output = captured(update_baseline, self.config)
        self.assertEqual(status, 0)
        self.assertEqual(self.recorded(), {"a.gd::busy": 4})
        self.assertIn("lowered cognitive-complexity a.gd::busy: 6 -> 4", output)
        self.assertIn("removed cognitive-complexity a.gd::gone: no longer exists", output)

    def test_init_records_every_violation(self):
        status, output = captured(update_baseline, self.config, init=True)
        self.assertEqual(status, 0)
        self.assertEqual(self.recorded(), {"a.gd::busy": 4, "b.gd::other": 3})
        self.assertIn("cognitive-complexity 2", output)
        text = self.config.baseline_path.read_text()
        self.assertTrue(text.endswith("}\n"))
        self.assertIn('  "version": 1', text)

    def test_init_refuses_to_overwrite(self):
        self.record({})
        status, output = captured(update_baseline, self.config, init=True)
        self.assertEqual(status, 1)
        self.assertIn("--force", output)
        self.assertEqual(captured(update_baseline, self.config, init=True, force=True)[0], 0)
        self.assertEqual(len(self.recorded()), 2)

    def test_unparsable_file_keeps_its_function_entries(self):
        self.record({"a.gd::busy": 4, "b.gd::other": 3})
        self.repo.write("b.gd", "func other(:\n")
        status, output = captured(check, self.config)
        self.assertIn("(parse-error)", output)
        self.assertNotIn("no longer exists", output)
        captured(update_baseline, self.config)
        self.assertEqual(self.recorded(), {"a.gd::busy": 4, "b.gd::other": 3})


class OtherRulesTest(unittest.TestCase):
    def test_long_comment_count_is_baselined_per_file(self):
        comment = "# one\n# two\n# three\n"
        source = "extends Node\nvar a := 1\n" + comment + "var b := 1\n" + comment + "var c := 1\n"
        repo = Repo({"c.gd": source})
        self.addCleanup(repo.cleanup)
        config = repo.config()
        status, output = captured(check, config)
        self.assertEqual(output.count("(long-comment)"), 2)
        captured(update_baseline, config, init=True)
        self.assertEqual(captured(check, config)[0], 0)
        repo.write("c.gd", source + comment + "var d := 1\n")
        status, output = captured(check, config)
        self.assertEqual(status, 1)
        self.assertIn("the file has 3 long comments, its baseline allows 2", output)

    def test_file_lines(self):
        repo = Repo({"long.gd": "extends Node\n" + "".join(f"var v{n} := {n}\n" for n in range(5))}, "limits:\n  file-lines: 3\n")
        self.addCleanup(repo.cleanup)
        status, output = captured(check, repo.config())
        self.assertIn("long.gd:1: 6 code lines (max 3) (file-lines)", output)


LAMBDAS = (
    "extends Node\n\n\nfunc f():\n"
    "\tvar old = func(x):\n" + "".join(f"\t\tif x == {n}:\n\t\t\treturn {n}\n" for n in range(4)) + "\t\treturn -1\n"
    "\tvar new = func(x):\n" + "".join(f"\t\tif x == {n}:\n\t\t\treturn {n}\n" for n in range(3)) + "\t\treturn -1\n"
    "\treturn [old, new]\n"
)


class FunctionKeyTest(unittest.TestCase):
    def test_each_lambda_is_judged_on_its_own(self):
        repo = Repo({"a.gd": LAMBDAS}, "limits:\n  cognitive: 100\n  cyclomatic: 2\n")
        self.addCleanup(repo.cleanup)
        config = repo.config()
        data = baseline.empty()
        data["cyclomatic-complexity"] = {"a.gd::f.<lambda#1>": 5}
        baseline.write(config.baseline_path, data)
        status, output = captured(check, config)
        self.assertEqual(status, 1)
        self.assertIn("a.gd:15: f.<lambda#2> has cyclomatic complexity 4 (max 2)", output)
        self.assertNotIn("<lambda#1>", output)
        self.assertEqual(captured(update_baseline, repo.config(), init=True, force=True)[0], 0)
        self.assertEqual(baseline.load(config.baseline_path)["cyclomatic-complexity"], {
            "a.gd::f.<lambda#1>": 5, "a.gd::f.<lambda#2>": 4,
        })

    def test_repeated_names_are_numbered(self):
        source = "extends Node\n\n\nfunc f():\n\tpass\n\n\nfunc f():\n\tpass\n"
        repo = Repo({"a.gd": source})
        self.addCleanup(repo.cleanup)
        items = analyze(repo.config(), "a.gd", Surfaces(repo.root)).items
        self.assertEqual([i.key for i in items if i.rule == "cyclomatic-complexity"], ["a.gd::f#1", "a.gd::f#2"])


class KeyPathTest(unittest.TestCase):
    def test_paths_holding_the_key_separator(self):
        self.assertEqual(baseline.item_path("cognitive-complexity", "a::b.gd::Inner.work"), "a::b.gd")
        self.assertEqual(baseline.item_path("file-lines", "a::b.gd"), "a::b.gd")

    def test_subset_check_reports_a_missing_function_in_such_a_file(self):
        repo = Repo({"a::b.gd": "extends Resource\n\n\nfunc calm():\n\tpass\n"}, CONFIG)
        self.addCleanup(repo.cleanup)
        config = repo.config()
        data = baseline.empty()
        data["cognitive-complexity"] = {"a::b.gd::work": 3}
        data["interface-cap"] = {"a::b.gd": 9}
        baseline.write(config.baseline_path, data)
        status, output = captured(check, config, [str(repo.root / "a::b.gd")])
        self.assertEqual(status, 1)
        self.assertIn("baseline entry 'a::b.gd::work' no longer exists", output)
        self.assertIn("baseline entry 'a::b.gd' no longer exists", output)


if __name__ == "__main__":
    unittest.main()
