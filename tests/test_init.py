from __future__ import annotations

import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

from godot_guard import config as config_module
from godot_guard.scaffold import REQUIREMENT, TEMPLATES, init

from .helpers import Repo, captured


class InitTest(unittest.TestCase):
    def setUp(self):
        self.repo = Repo({"tests/run_tests.gd": "extends SceneTree\n", "tools/requirements-lint.txt": "gdtoolkit==4.5.0\n"})
        self.addCleanup(self.repo.cleanup)
        self.repo.write("project.godot", 'config/features=PackedStringArray("4.7", "GL Compatibility")\n')
        subprocess.run(["git", "init", "-q"], cwd=self.repo.root, check=True)

    def test_scaffolds_everything_once(self):
        status, output = captured(init, self.repo.root)
        self.assertEqual(status, 0)
        root = self.repo.root
        config = config_module.load(root)
        self.assertEqual(config.gate["tests"], [config_module.TestScript("res://tests/run_tests.gd")])
        self.assertTrue(os.access(root / ".githooks" / "pre-commit", os.X_OK))
        self.assertIn("gate --fast --staged", (root / ".githooks" / "pre-commit").read_text())
        self.assertIn("version: 4.7.1", (root / ".github" / "workflows" / "quality.yml").read_text())
        self.assertEqual((root / "tools" / "requirements-lint.txt").read_text(), f"gdtoolkit==4.5.0\n{REQUIREMENT}\n")
        hooks = subprocess.run(["git", "config", "core.hooksPath"], cwd=root, capture_output=True, text=True)
        self.assertEqual(hooks.stdout.strip(), ".githooks")
        self.assertIn("ran git config core.hooksPath .githooks", output)
        status, output = captured(init, root)
        self.assertEqual(output.count("skipped"), 5)

    def test_newer_minor_version_is_used_as_is(self):
        self.repo.write("project.godot", 'config/features=PackedStringArray("4.8", "Forward Plus")\n')
        captured(init, self.repo.root)
        self.assertIn("version: 4.8\n", (self.repo.root / ".github" / "workflows" / "quality.yml").read_text())


HOST_CHECK = """#!/bin/sh
if [ "${HOST_CHECK_ACTIVE:-}" = 1 ]; then echo nested >> "$LOG"; exit 64; fi
echo "host-check $*" >> "$LOG"
HOST_CHECK_ACTIVE=1 exec "$@"
"""
GUARD = '#!/bin/sh\necho "godot-guard $*" >> "$LOG"\n'


class HookTest(unittest.TestCase):
    """The hook templates run under a fake host-check that refuses nesting, as the real one does."""

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        for name, text in (("host-check", HOST_CHECK), ("godot-guard", GUARD)):
            path = self.root / name
            path.write_text(text)
            path.chmod(path.stat().st_mode | stat.S_IEXEC)
        self.log = self.root / "calls.log"

    def hook(self, name: str, active: str | None) -> tuple[int, list[str]]:
        environment = dict(os.environ, PATH=f"{self.root}{os.pathsep}{os.environ['PATH']}", LOG=str(self.log))
        environment.pop("HOST_CHECK_ACTIVE", None)
        if active is not None:
            environment["HOST_CHECK_ACTIVE"] = active
        status = subprocess.run(["sh", str(TEMPLATES / name)], cwd=self.root, env=environment).returncode
        return status, self.log.read_text().splitlines()

    def test_hooks_queue_through_host_check(self):
        self.assertEqual(self.hook("pre-commit", None), (0, ["host-check godot-guard gate --fast --staged", "godot-guard gate --fast --staged"]))

    def test_hooks_inside_a_host_check_job_run_directly(self):
        self.assertEqual(self.hook("pre-push", "1"), (0, ["godot-guard gate"]))


if __name__ == "__main__":
    unittest.main()
