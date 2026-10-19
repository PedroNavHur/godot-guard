from __future__ import annotations

import contextlib
import io
import json
import os
import unittest

from godot_guard import baseline
from godot_guard.check import check
from godot_guard.cli import main

from .helpers import Repo, captured

CONFIG = "exclude: [vendor]\nlimits:\n  cognitive: 2\n  comment-lines: 3\nrules:\n  cyclomatic-complexity: warn\n  nested-ternary: off\n"
BUSY = "extends Node\n\n\nfunc busy(x: int) -> int:\n" + "".join(
    f"\tif x == {n}:\n\t\treturn {n}\n" for n in range(4)
) + "\treturn -1\n"


class CheckJsonTest(unittest.TestCase):
    def setUp(self):
        self.repo = Repo({"a.gd": BUSY, "b.gd": "extends Node\n", "vendor/c.gd": BUSY}, CONFIG)
        self.addCleanup(self.repo.cleanup)
        self.config = self.repo.config()

    def run_json(self, files=None) -> tuple[int, dict, str]:
        errors = io.StringIO()
        with contextlib.redirect_stderr(errors):
            status, output = captured(check, self.config, files, as_json=True)
        return status, json.loads(output), errors.getvalue()

    def test_reports_settings_files_and_problems(self):
        status, report, _ = self.run_json()
        self.assertEqual(status, 1)
        self.assertEqual(report["version"], 1)
        self.assertEqual(report["root"], str(self.repo.root))
        self.assertEqual(
            report["limits"], {"cognitive": 2, "cyclomatic": 16, "comment-lines": 3, "inputs": 8}
        )
        self.assertEqual(report["levels"]["cyclomatic-complexity"], "warn")
        self.assertEqual(report["levels"]["nested-ternary"], "off")
        self.assertEqual(report["levels"]["long-comment"], "error")
        self.assertEqual(report["files"], ["a.gd", "b.gd"])
        self.assertEqual(report["problems"], [{
            "path": "a.gd", "line": 4, "rule": "cognitive-complexity", "level": "error",
            "message": "busy has cognitive complexity 4 (max 2)",
        }])

    def test_problems_match_the_text_output(self):
        self.repo.write("d.gd", "extends Node\n\n\nfunc f(a, b):\n\treturn 1 if a else (2 if b else 3)\n")
        self.repo.write(".godot-guard.yml", "limits:\n  cyclomatic: 1\nrules:\n  cyclomatic-complexity: warn\n")
        self.config = self.repo.config()
        status, report, _ = self.run_json()
        text_status, text = captured(check, self.config)
        self.assertEqual(status, text_status)
        lines = [line for line in text.splitlines() if not line.startswith("godot-guard check:")]
        printed = [
            f"{p['path']}:{p['line']}: {'warning: ' if p['level'] == 'warn' else ''}{p['message']} ({p['rule']})"
            for p in report["problems"]
        ]
        self.assertEqual(printed, lines)
        self.assertIn("warn", {p["level"] for p in report["problems"]})

    def test_warnings_alone_exit_zero(self):
        self.repo.write(".godot-guard.yml", "limits:\n  cognitive: 2\nrules:\n  cognitive-complexity: warn\n")
        self.config = self.repo.config()
        status, report, _ = self.run_json([str(self.repo.root / "a.gd")])
        self.assertEqual(status, 0)
        self.assertEqual([p["level"] for p in report["problems"]], ["warn"])

    def test_loose_and_stale_baseline_entries_are_problems(self):
        data = baseline.empty()
        data["cognitive-complexity"] = {"a.gd::busy": 5, "a.gd::gone": 9, "b.gd::gone": 9}
        baseline.write(self.config.baseline_path, data)
        status, report, _ = self.run_json([str(self.repo.root / "a.gd")])
        self.assertEqual(status, 1)
        self.assertEqual(report["files"], ["a.gd"])
        self.assertEqual([(p["path"], p["line"], p["rule"]) for p in report["problems"]], [
            (".godot-guard-baseline.json", 4, "cognitive-complexity"),
            ("a.gd", 4, "cognitive-complexity"),
        ])
        self.assertIn("below its baseline 5", report["problems"][1]["message"])

    def test_skipped_arguments_stay_on_stderr(self):
        status, report, errors = self.run_json([str(self.repo.root / "vendor" / "c.gd")])
        self.assertEqual(status, 0)
        self.assertEqual((report["files"], report["problems"]), ([], []))
        self.assertIn("skipping vendor/c.gd: it matches exclude", errors)

    def test_command_line_flag(self):
        previous = os.getcwd()
        os.chdir(self.repo.root)
        self.addCleanup(os.chdir, previous)
        status, output = captured(main, ["check", "--json", "b.gd"])
        self.assertEqual(status, 0)
        self.assertEqual(json.loads(output)["files"], ["b.gd"])


if __name__ == "__main__":
    unittest.main()
