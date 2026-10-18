from __future__ import annotations

import contextlib
import io
import os
import stat
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from godot_guard import config as config_module, gate as gate_module
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

    def test_staged_files_are_checked_even_when_deleted_from_the_working_tree(self):
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 0, output)
        self.assertEqual(self.calls(), ["gdformat --check a.gd b.gd", "gdlint a.gd b.gd"])
        self.assertIn("0 errors, 0 warnings, 2 files", output)
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
        self.assertEqual(self.calls(), ["gdformat --check a.gd b.gd"])


RESOURCE_BASE = "class_name Base\nextends Resource\n"
WIDE = "extends Base\n\n" + "".join(f"@export var v{n} := 1\n" for n in range(9))


class StagedContentTest(ShimTest):
    """The fast staged gate checks the index's content, not the working tree's."""

    def setUp(self):
        super().setUp()
        git(self.repo.root, "init", "-q")
        self.repo.write(".gdlintrc", "max-line-length: 100\n")
        self.shim("gdformat", extra=f'cat "$2" >> "{self.log}"')
        self.shim("gdlint", extra=f'cat .gdlintrc >> "{self.log}"')

    def stage(self, name: str, staged: str, working: str) -> None:
        self.repo.write(name, staged)
        git(self.repo.root, "add", name)
        self.repo.write(name, working)

    def test_invalid_staged_content_fails_with_a_clean_working_copy(self):
        self.stage("a.gd", "func f(:\n", CLEAN)
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 1)
        self.assertEqual(self.calls(), ["gdformat --check a.gd", "func f(:", "gdlint a.gd", "max-line-length: 100"])
        self.assertIn("a.gd:1: cannot parse", output)

    def test_valid_staged_content_passes_with_a_broken_working_copy(self):
        self.stage("a.gd", CLEAN, "func f(:\n")
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 0, output)

    def test_resource_exclusion_reads_the_index_for_selected_scripts(self):
        self.stage("base.gd", "class_name Base\nextends Node\n", "class_name Base\nextends Resource\n")
        self.stage("wide.gd", WIDE, WIDE)
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 1)
        self.assertIn("wide.gd:1: 9 inputs (@export 9) (max 8) (interface-cap)", output)

    def restage_base(self, delete_old: bool, add_new: bool) -> tuple[int, str]:
        """Commit z_old.gd declaring `Base extends Resource` and an `extends Base` script, restage, and gate."""
        self.stage("z_old.gd", RESOURCE_BASE, RESOURCE_BASE)
        self.stage("wide.gd", WIDE, WIDE)
        git(self.repo.root, "commit", "-qm", "base")
        if delete_old:
            git(self.repo.root, "rm", "-q", "--cached", "z_old.gd")
        if add_new:
            self.stage("a_new.gd", "class_name Base\nextends Node\n", "class_name Base\nextends Node\n")
        self.stage("wide.gd", WIDE + "\n", WIDE + "\n")
        return captured(gate_module.gate, self.repo.config(), True, True, [])

    def test_staged_class_name_beats_the_working_tree(self):
        status, output = self.restage_base(delete_old=False, add_new=True)
        self.assertEqual(status, 1, output)
        self.assertIn("wide.gd:1: 9 inputs", output)

    def test_scripts_staged_for_deletion_leave_the_resource_map(self):
        status, output = self.restage_base(delete_old=True, add_new=False)
        self.assertEqual(status, 1, output)
        self.assertIn("wide.gd:1: 9 inputs", output)

    def test_staged_deletion_and_new_declaration_of_a_class(self):
        status, output = self.restage_base(delete_old=True, add_new=True)
        self.assertEqual(status, 1, output)
        self.assertIn("wide.gd:1: 9 inputs", output)

    def test_resource_exclusion_reads_the_working_tree_for_unselected_scripts(self):
        source = self.repo.root / ".source"
        source.mkdir()
        (source / "base.gd").write_text("class_name Base\nextends Resource\n")
        git(source, "init", "-q")
        git(source, "add", "base.gd")
        git(source, "commit", "-qm", "base")
        git(self.repo.root, "-c", "protocol.file.allow=always", "submodule", "add", "-q", str(source), "vendor")
        self.stage("wide.gd", WIDE, WIDE)
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 0, output)
        self.assertIn("0 errors, 0 warnings, 1 file", output)

    def test_unchanged_symlink_to_a_non_script_is_ignored(self):
        self.repo.write("target.txt", CLEAN)
        (self.repo.root / "alias.gd").symlink_to("target.txt")
        git(self.repo.root, "add", "target.txt", "alias.gd")
        git(self.repo.root, "commit", "-qm", "alias")
        self.stage("a.gd", CLEAN, CLEAN)
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 0, output)
        self.assertIn("0 errors, 0 warnings, 1 file", output)

    def test_staged_symlinked_scripts_fail_the_gate(self):
        self.stage("target.gd", CLEAN, CLEAN)
        self.stage("retyped.gd", CLEAN, CLEAN)
        git(self.repo.root, "commit", "-qm", "scripts")
        (self.repo.root / "retyped.gd").unlink()
        for name in ("alias.gd", "retyped.gd"):
            (self.repo.root / name).symlink_to("target.gd")
        git(self.repo.root, "add", "alias.gd", "retyped.gd")
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 1, output)
        message = "{}: symlinked scripts are not supported; commit the real file"
        self.assertEqual(output.splitlines(), [message.format("alias.gd"), message.format("retyped.gd")])
        self.assertEqual(self.calls(), [])

    def test_baseline_comes_from_the_index(self):
        self.stage("wide.gd", WIDE.replace("Base", "Node"), WIDE.replace("Base", "Node"))
        self.stage(".godot-guard-baseline.json", '{"interface-cap": {"wide.gd": 9}}\n', '{"interface-cap": {}}\n')
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 0, output)

    def test_config_root_below_the_git_root(self):
        game = self.repo.root / "game"
        self.repo.write("game/project.godot", "config_version=5\n")
        self.repo.write("game/.godot-guard.yml", "paths: [.]\n")
        self.stage("game/scripts/a.gd", "func f(:\n", CLEAN)
        self.stage("other.gd", "func f(:\n", CLEAN)
        status, output = captured(gate_module.gate, config_module.load(game), True, True, [])
        self.assertEqual(status, 1)
        self.assertEqual(self.calls()[0], "gdformat --check scripts/a.gd")
        self.assertIn("scripts/a.gd:1: cannot parse", output)
        self.assertIn("1 file", output)


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


class ShardedTestsTest(ShimTest):
    def test_sharded_entries_run_one_process_per_shard(self):
        self.repo.write(".godot-guard.yml", (
            "gate:\n  format: false\n  lint: false\n  import: false\n  analyzer: false\n  smoke: false\n"
            "  tests: [{script: res://tests/run_tests.gd, shards: 2}]\n"
        ))
        status, output = captured(gate_module.gate, self.repo.config(), False, False, [])
        self.assertEqual(status, 0, output)
        root = str(self.repo.root)
        self.assertEqual(self.calls(), [
            f"godot --headless --path {root} --script res://tests/run_tests.gd -- --shard=1/2",
            f"godot --headless --path {root} --script res://tests/run_tests.gd -- --shard=2/2",
        ])
        self.assertIn("== tests res://tests/run_tests.gd [2/2]", output)


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

    def test_hidden_or_excluded_arguments_select_nothing(self):
        names = ["scripts/.hidden", "scripts/.hidden/f.gd", "scripts/gen", "scripts/gen/e.gd"]
        errors = io.StringIO()
        with contextlib.redirect_stderr(errors):
            status, output = captured(format_files, self.repo.config(), [str(self.repo.root / n) for n in names])
        self.assertEqual(status, 0, output)
        self.assertIn("no GDScript files selected", output)
        self.assertEqual(self.calls(), [])
        for name in names:
            self.assertIn(f"skipping {name}:", errors.getvalue())
        self.assertIn("scripts/.hidden: hidden directories are not scanned", errors.getvalue())
        self.assertIn("scripts/gen: it matches exclude", errors.getvalue())

    def test_links_in_hidden_or_excluded_directories_select_nothing(self):
        links = {".hidden/alias.gd": "../a.gd", ".hidden/directory": "../scripts", "scripts/gen/alias.gd": "../d.gd"}
        for name, target in links.items():
            (self.repo.root / name).parent.mkdir(exist_ok=True)
            (self.repo.root / name).symlink_to(target)
        names = [".hidden/alias.gd", ".hidden/directory", ".hidden/directory/d.gd", "scripts/gen/alias.gd"]
        errors = io.StringIO()
        with contextlib.redirect_stderr(errors):
            status, output = captured(format_files, self.repo.config(), [str(self.repo.root / n) for n in names])
        self.assertEqual(status, 0, output)
        self.assertEqual(self.calls(), [])
        for name in names:
            self.assertIn(f"skipping {name}:", errors.getvalue())

    def test_staged_scripts_in_hidden_directories_are_not_selected(self):
        git(self.repo.root, "init", "-q")
        git(self.repo.root, "add", "scripts/.hidden/f.gd")
        status, output = captured(gate_module.gate, self.repo.config(), True, True, [])
        self.assertEqual(status, 0, output)
        self.assertIn("no GDScript files selected", output)


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

    def test_captured_output_is_echoed_and_warnings_still_fail(self):
        self.shim("godot", extra="echo 'WARNING: unused variable'")
        lines: list[str] = []
        self.assertEqual(run_godot([], echo=lines.append), 1)
        self.assertEqual(lines, ["WARNING: unused variable\n"])

    def test_captured_clean_output_passes_and_exit_status_is_kept(self):
        self.shim("godot", extra="echo 'Godot Engine v4.7.1'")
        lines: list[str] = []
        self.assertEqual(run_godot(["--version"], echo=lines.append), 0)
        self.assertEqual(lines, ["Godot Engine v4.7.1\n"])
        self.shim("godot", status=3, extra="echo 'bye'")
        self.assertEqual(run_godot([], echo=lines.append), 3)
        self.assertEqual(lines[-1], "bye\n")


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
