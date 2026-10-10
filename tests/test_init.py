from __future__ import annotations

import os
import subprocess
import unittest

from godot_guard import config as config_module
from godot_guard.scaffold import REQUIREMENT, init

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
        self.assertEqual(config.gate["tests"], ["res://tests/run_tests.gd"])
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


if __name__ == "__main__":
    unittest.main()
