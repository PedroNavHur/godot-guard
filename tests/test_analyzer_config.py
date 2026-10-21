from __future__ import annotations

import json
import unittest

from godot_guard import analyzer_config
from godot_guard.check import check
from godot_guard.config import ConfigError

from .fixtures import HEXSTEAD_WARNINGS
from .helpers import Repo, captured, strict_project

SCRIPT = {"scripts/a.gd": "extends Node\n"}


def project(*overrides: str) -> str:
    """The strict project with each `key=value` in `overrides` replacing that key's setting."""
    keys = {line.split("=", 1)[0] for line in overrides}
    kept = [line for line in strict_project().splitlines() if line.split("=", 1)[0] not in keys]
    return "\n".join([*kept, *overrides]) + "\n"


def hexstead_project(skipped: str = "") -> str:
    """A project with hexstead's warnings each at error, except `skipped`."""
    settings = "".join(f"gdscript/warnings/{name}=2\n" for name in HEXSTEAD_WARNINGS if name != skipped)
    return "config_version=5\n\n[debug]\n\n" + settings


class AnalyzerConfigTest(unittest.TestCase):
    def problems(self, project_text: str, config_text: str | None = None) -> list:
        repo = Repo({**SCRIPT, "project.godot": project_text}, config_text)
        self.addCleanup(repo.cleanup)
        return analyzer_config.problems(repo.config())

    def test_every_required_warning_at_error_passes(self):
        self.assertEqual(self.problems(strict_project()), [])

    def test_a_bare_project_lists_the_warnings_below_error_by_default(self):
        [problem] = self.problems("config_version=5\n")
        self.assertEqual((problem.path, problem.line, problem.rule), ("project.godot", 1, "analyzer-config"))
        self.assertIn("untyped_declaration", problem.message)
        self.assertIn("unused_variable", problem.message)
        self.assertNotIn("native_method_override", problem.message)

    def test_the_missing_list_points_at_the_debug_header(self):
        [problem] = self.problems("config_version=5\n\n[debug]\n\ngdscript/warnings/unused_variable=2\n")
        self.assertEqual(problem.line, 3)
        self.assertNotIn("unused_variable", problem.message)

    def test_a_warning_below_error_is_reported_on_its_line(self):
        text = project("gdscript/warnings/unsafe_cast=1")
        line = text.splitlines().index("gdscript/warnings/unsafe_cast=1") + 1
        [problem] = self.problems(text)
        self.assertEqual(problem.line, line)
        self.assertIn("gdscript/warnings/unsafe_cast is 1", problem.message)

    def test_lowering_a_warning_godot_defaults_to_error_is_reported(self):
        [problem] = self.problems(project("gdscript/warnings/native_method_override=1"))
        self.assertIn("native_method_override is 1", problem.message)

    def test_turning_warnings_off_is_reported(self):
        [problem] = self.problems(project("gdscript/warnings/enable=false"))
        self.assertIn("gdscript/warnings/enable", problem.message)

    def test_settings_outside_the_debug_section_do_not_count(self):
        text = "config_version=5\n\n[application]\n\ngdscript/warnings/untyped_declaration=2\n"
        [problem] = self.problems(text)
        self.assertIn("untyped_declaration", problem.message)

    def test_relaxed_warnings_are_not_checked(self):
        config_text = "relaxed-warnings: [unsafe_cast, untyped_declaration]\n"
        text = project("gdscript/warnings/unsafe_cast=1", "gdscript/warnings/untyped_declaration=0")
        self.assertEqual(self.problems(text, config_text), [])

    def test_a_relaxed_name_outside_the_required_set_is_a_config_error(self):
        repo = Repo(SCRIPT, "relaxed-warnings: [inferred_declaration]\n")
        self.addCleanup(repo.cleanup)
        with self.assertRaisesRegex(ConfigError, "inferred_declaration"):
            repo.config()

    def test_hexsteads_settings_at_error_pass(self):
        self.assertEqual(self.problems(hexstead_project()), [])

    def test_dropping_one_of_hexsteads_settings_names_it(self):
        [problem] = self.problems(hexstead_project(skipped="unused_variable"))
        self.assertIn("unused_variable", problem.message)

    def test_default_addons_directory_rule_passes(self):
        self.assertEqual(self.problems(project('gdscript/warnings/directory_rules={"res://addons": 0}')), [])
        multi_line = project('gdscript/warnings/directory_rules={\n"res://addons": 0\n}')
        self.assertEqual(self.problems(multi_line), [])

    def test_a_directory_rule_turning_warnings_off_under_scripts_is_reported(self):
        text = project('gdscript/warnings/directory_rules={\n"res://addons": 0,\n"res://scripts": 0\n}')
        [problem] = self.problems(text)
        self.assertEqual(problem.line, text.splitlines().index("gdscript/warnings/directory_rules={") + 1)
        self.assertIn("res://scripts", problem.message)

    def test_a_one_line_directory_rule_under_scripts_is_reported(self):
        [problem] = self.problems(project('gdscript/warnings/directory_rules={"res://scripts": 0}'))
        self.assertIn("res://scripts", problem.message)

    def test_a_directory_rule_that_keeps_warnings_passes(self):
        self.assertEqual(self.problems(project('gdscript/warnings/directory_rules={"res://scripts": 1}')), [])


class CheckWiringTest(unittest.TestCase):
    def repo(self, config_text: str | None = None) -> Repo:
        repo = Repo({**SCRIPT, "project.godot": "config_version=5\n"}, config_text)
        self.addCleanup(repo.cleanup)
        return repo

    def test_a_full_check_fails_on_weak_settings(self):
        status, output = captured(check, self.repo().config())
        self.assertEqual(status, 1)
        self.assertIn("project.godot:1: ", output)
        self.assertIn("(analyzer-config)", output)

    def test_checking_named_files_leaves_project_godot_alone(self):
        repo = self.repo()
        status, output = captured(check, repo.config(), [str(repo.root / "scripts/a.gd")])
        self.assertEqual((status, "analyzer-config" in output), (0, False))

    def test_warn_level_passes_and_off_is_silent(self):
        status, output = captured(check, self.repo("rules:\n  analyzer-config: warn\n").config())
        self.assertEqual(status, 0)
        self.assertIn("warning: ", output)
        status, output = captured(check, self.repo("rules:\n  analyzer-config: off\n").config())
        self.assertEqual((status, "analyzer-config" in output), (0, False))

    def test_json_reports_the_problem(self):
        _, output = captured(check, self.repo().config(), as_json=True)
        rules = [p["rule"] for p in json.loads(output)["problems"]]
        self.assertEqual(rules, ["analyzer-config"])


if __name__ == "__main__":
    unittest.main()
