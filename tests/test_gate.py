from __future__ import annotations

import os
import stat
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from godot_guard import gate as gate_module
from godot_guard.cli import format_files, main
from godot_guard.tools import ToolError, godot, run_godot

from .helpers import Repo, captured

SHIM = """#!/bin/sh
echo "$(basename "$0") $*" >> "{log}"
{extra}
exit {status}
"""
CLEAN = "extends Node\n\nvar a := 1\n"


def git(root: Path, *args: str) -> None:
    subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)


class ShimTest(unittest.TestCase):
    """A temporary repository with fake gdformat, gdlint, and Godot on PATH."""

    def setUp(self):
        self.repo = Repo({"a.gd": CLEAN, "b.gd": CLEAN, "c.gd": CLEAN}, "paths: [.]\n")
        self.addCleanup(self.repo.cleanup)
        self.bin = self.repo.root / ".bin"
        self.log = self.repo.root / ".bin" / "calls.log"
        for name in ("gdformat", "gdlint", "godot"):
            self.shim(name)
        environment = {"PATH": f"{self.bin}{os.pathsep}{os.environ['PATH']}", "GODOT": str(self.bin / "godot")}
        patches = [mock.patch.dict(os.environ, environment), mock.patch("godot_guard.tools.sys.executable", "/nonexistent/python")]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

    def shim(self, name: str, status: int = 0, extra: str = "") -> None:
        path = self.repo.write(f".bin/{name}", SHIM.format(log=self.log, extra=extra, status=status))
        path.chmod(path.stat().st_mode | stat.S_IEXEC)

    def calls(self) -> list[str]:
        return self.log.read_text().splitlines() if self.log.exists() else []


class FastGateTest(ShimTest):
    def setUp(self):
        super().setUp()
        git(self.repo.root, "init", "-q")
        git(self.repo.root, "add", "a.gd", "b.gd")
        (self.repo.root / "b.gd").unlink()

    def test_staged_files_that_exist(self):
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 0, output)
        self.assertEqual(self.calls(), ["gdformat --check a.gd", "gdlint a.gd"])
        self.assertIn("0 errors, 0 warnings, 1 file", output)
        self.assertIn("== check passed in", output)

    def test_nothing_staged_is_nothing_to_do(self):
        git(self.repo.root, "rm", "-q", "--cached", "a.gd", "b.gd")
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 0)
        self.assertIn("no GDScript files selected", output)
        self.assertEqual(self.calls(), [])

    def test_failing_format_stops_the_gate(self):
        self.shim("gdformat", status=1)
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 1)
        self.assertIn("== format failed", output)
        self.assertEqual(self.calls(), ["gdformat --check a.gd"])


class FullGateTest(ShimTest):
    def test_runs_every_step_in_order(self):
        self.repo.write(".godot-guard.yml", (
            "paths: [scripts, .]\nexclude: [addons/**]\n"
            "gate:\n  steps: ['echo step >> .bin/calls.log']\n  tests: [res://tests/run_tests.gd]\n"
        ))
        status, output = captured(gate_module.gate, self.repo.config(), False, False, [])
        self.assertEqual(status, 0, output)
        analyzer = str(gate_module.ANALYZER)
        root = str(self.repo.root)
        self.assertEqual(self.calls(), [
            "gdformat --check a.gd b.gd c.gd",
            "gdlint a.gd b.gd c.gd",
            "step",
            f"godot --headless --path {root} --editor --quit",
            f"godot --headless --path {root} --script {analyzer} -- --root res://scripts --root res:// --exclude addons/**",
            f"godot --headless --path {root} --script res://tests/run_tests.gd",
            f"godot --headless --path {root} --quit-after 3",
        ])
        self.assertTrue(gate_module.ANALYZER.is_file())

    def test_godot_warning_fails_the_gate(self):
        self.shim("godot", extra="echo 'WARNING: something odd'")
        status, output = captured(gate_module.gate, self.repo.config(), False, False, [])
        self.assertEqual(status, 1)
        self.assertIn("== import failed", output)


class DirectoryArgumentTest(ShimTest):
    def setUp(self):
        super().setUp()
        for name in ("scripts/d.gd", "scripts/gen/e.gd", "scripts/.hidden/f.gd", "scripts/notes.txt"):
            self.repo.write(name, CLEAN)
        self.repo.write(".godot-guard.yml", "paths: [.]\nexclude: [scripts/gen]\n")

    def test_format_expands_directories(self):
        status, output = captured(format_files, self.repo.config(), [str(self.repo.root / "scripts"), str(self.repo.root / "a.gd")])
        self.assertEqual(status, 0, output)
        self.assertEqual(self.calls(), ["gdformat a.gd scripts/d.gd"])

    def test_fast_gate_expands_directories(self):
        status, output = captured(gate_module.gate, self.repo.config(), True, False, [str(self.repo.root / "scripts")])
        self.assertEqual(status, 0, output)
        self.assertEqual(self.calls(), ["gdformat --check scripts/d.gd", "gdlint scripts/d.gd"])


class RunTest(ShimTest):
    def test_warning_with_exit_zero_fails(self):
        self.shim("godot", extra="echo 'WARNING: unused variable'")
        self.assertEqual(captured(run_godot, ["--version"])[0], 1)

    def test_script_error_anywhere_in_a_line_fails(self):
        self.shim("godot", extra="echo '  SCRIPT ERROR: Parse Error'")
        self.assertEqual(captured(run_godot, [])[0], 1)

    def test_clean_output_passes_and_exit_status_is_kept(self):
        self.shim("godot", extra="echo 'Godot Engine v4.7.1'")
        self.assertEqual(captured(main, ["run", "--", "--headless", "--version"])[0], 0)
        self.assertEqual(self.calls(), ["godot --headless --version"])
        self.shim("godot", status=3)
        self.assertEqual(captured(run_godot, [])[0], 3)


def real_godot() -> str | None:
    try:
        return godot()
    except ToolError:
        return None


@unittest.skipUnless(real_godot(), "Godot 4 not found")
class AnalyzerTest(unittest.TestCase):
    """The packaged analyzer script under a real headless Godot."""

    def setUp(self):
        self.repo = Repo({"a.gd": CLEAN, "scripts/b.gd": CLEAN, "empty/notes.txt": ""})
        self.addCleanup(self.repo.cleanup)

    def analyze(self, *roots: str) -> tuple[int, str]:
        args = [arg for root in roots for arg in ("--root", root)]
        return captured(run_godot, ["--headless", "--path", str(self.repo.root), "--script", str(gate_module.ANALYZER), "--", *args])

    def test_file_roots_are_analyzed(self):
        status, output = self.analyze("res://a.gd")
        self.assertEqual(status, 0, output)
        self.assertIn("Analyzed 1 scripts, 0 failed to load.", output)

    def test_missing_root_fails(self):
        status, output = self.analyze("res://missing", "res://a.gd")
        self.assertEqual(status, 1, output)
        self.assertIn("ANALYZER: root res://missing does not exist", output)

    def test_roots_without_scripts_fail(self):
        status, output = self.analyze("res://empty")
        self.assertEqual(status, 1, output)
        self.assertIn("ANALYZER: no scripts under res://empty", output)


if __name__ == "__main__":
    unittest.main()
