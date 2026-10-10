from __future__ import annotations

import unittest

from godot_guard import config as config_module
from godot_guard.check import check

from .helpers import Repo, captured

NESTED = "extends Node\n\n\nfunc f(a: bool) -> int:\n\treturn 1 if a else 2 if not a else 3\n"


class ConfigTest(unittest.TestCase):
    def repo(self, files: dict, text: str | None) -> Repo:
        repo = Repo(files, text)
        self.addCleanup(repo.cleanup)
        return repo

    def test_defaults_without_a_config_file(self):
        config = self.repo({}, None).config()
        self.assertEqual(config.paths, ["."])
        self.assertEqual(config.limits["cognitive"], 8)
        self.assertEqual(config.gate["smoke"], ["--quit-after", "3"])
        self.assertEqual(config.rules["nested-ternary"], "warn")

    def test_partial_sections_merge_with_defaults(self):
        config = self.repo({}, "limits:\n  cognitive: 6\ngate:\n  smoke: false\n").config()
        self.assertEqual(config.limits["cognitive"], 6)
        self.assertEqual(config.limits["cyclomatic"], 16)
        self.assertFalse(config.gate["smoke"])

    def test_unknown_keys_are_errors(self):
        for text, key in (("limit:\n  cognitive: 6\n", "'limit'"), ("limits:\n  cognitve: 6\n", "'limits.cognitve'")):
            with self.assertRaises(config_module.ConfigError) as caught:
                self.repo({}, text).config()
            self.assertIn(key, str(caught.exception))

    def test_wrong_shapes_are_errors(self):
        cases = (
            ("false\n", "the file must be a mapping"), ("[]\n", "the file must be a mapping"),
            ("limits: false\n", "limits must be a mapping"), ("rules: []\n", "rules must be a mapping"),
            ("gate: 0\n", "gate must be a mapping"), ("boundaries: false\n", "boundaries must be a list"),
            ("paths: false\n", "paths must be a list of strings"), ("exclude: {}\n", "exclude must be a list of strings"),
        )
        for text, message in cases:
            with self.assertRaises(config_module.ConfigError, msg=text) as caught:
                self.repo({}, text).config()
            self.assertIn(message, str(caught.exception))

    def test_null_takes_the_default(self):
        text = "paths: null\nlimits: ~\nrules:\n  file-lines: null\ngate:\n  smoke: null\nboundaries:\n"
        config = self.repo({}, text).config()
        self.assertEqual((config.paths, config.limits["cognitive"]), (["."], 8))
        self.assertEqual((config.rules["file-lines"], config.gate["smoke"], config.boundaries), ("error", ["--quit-after", "3"], []))
        self.assertEqual(self.repo({}, "").config().paths, ["."])

    def test_duplicate_keys_are_errors(self):
        cases = (
            ("limits: {cognitve: 1}\nlimits: {cognitive: 100}\n", "duplicate key 'limits'", "line 2"),
            ("gate:\n  format: true\n  lint: true\n  format: false\n", "duplicate key 'format'", "line 4"),
            ("boundaries:\n  - name: a\n    forbid: [x]\n    name: b\n", "duplicate key 'name'", "line 4"),
        )
        for text, message, line in cases:
            with self.assertRaises(config_module.ConfigError, msg=text) as caught:
                self.repo({}, text).config()
            self.assertIn(message, str(caught.exception))
            self.assertIn(line, str(caught.exception))

    def test_bad_level_is_an_error(self):
        with self.assertRaises(config_module.ConfigError):
            self.repo({}, "rules:\n  file-lines: fatal\n").config()
        with self.assertRaises(config_module.ConfigError):
            self.repo({}, "rules:\n  nested-ternary: error\n").config()

    def test_rule_off_silences_it(self):
        repo = self.repo({"n.gd": NESTED}, "rules:\n  nested-ternary: off\n")
        status, output = captured(check, repo.config())
        self.assertEqual(status, 0)
        self.assertNotIn("nested-ternary", output)

    def test_warn_level_prints_and_passes(self):
        repo = self.repo({"n.gd": NESTED}, "limits:\n  cognitive: 1\nrules:\n  cognitive-complexity: warn\n")
        status, output = captured(check, repo.config())
        self.assertEqual(status, 0)
        self.assertIn("n.gd:5: warning: nested conditional expression", output)
        self.assertIn("warning: f has cognitive complexity", output)
        self.assertIn("0 errors, 2 warnings, 1 file", output)

    def test_paths_and_exclude_select_scripts(self):
        files = {"scripts/a.gd": "", "scripts/gen/b.gd": "", "addons/x/c.gd": "", "other/d.gd": "", ".hidden/e.gd": ""}
        repo = self.repo(files, "paths: [./scripts, addons]\nexclude: [scripts/gen]\n")
        self.assertEqual(repo.config().scanned_files(), ["addons/x/c.gd", "scripts/a.gd"])
        self.assertEqual(self.repo(files, None).config().scanned_files(), ["other/d.gd", "scripts/a.gd", "scripts/gen/b.gd"])

    def test_root_is_found_from_a_subdirectory(self):
        repo = self.repo({"scripts/a.gd": ""}, "paths: [scripts]\n")
        self.assertEqual(config_module.load(repo.root / "scripts").root, repo.root)


if __name__ == "__main__":
    unittest.main()
