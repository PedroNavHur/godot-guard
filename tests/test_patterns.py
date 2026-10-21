from __future__ import annotations

import unittest

from godot_guard import patterns
from godot_guard.baseline import load
from godot_guard.check import check, update_baseline
from godot_guard.config import ConfigError
from godot_guard.rules import parse_tree

from . import fixtures
from .helpers import Repo, captured


def lines(source: str) -> dict[str, list[int]]:
    return {rule: [line for line, _ in hits] for rule, hits in patterns.scan(parse_tree(source)).items()}


class PatternTest(unittest.TestCase):
    def setUp(self):
        self.found = lines(fixtures.HOT_PATH)

    def test_lookups_in_hot_functions_but_not_in_ready_or_lambdas(self):
        self.assertEqual(self.found["lookup-in-hot-path"], [12, 13, 14, 15, 24, 29, 41])

    def test_loads_in_hot_functions(self):
        self.assertEqual(self.found["load-in-hot-path"], [16, 17])

    def test_allocations_only_in_frame_functions(self):
        self.assertEqual(self.found["per-frame-allocation"], [18, 19])

    def test_exported_variables_named_private(self):
        self.assertEqual(self.found["private-export"], [3, 6, 38])

    def test_plain_self_assignment_but_not_compound(self):
        self.assertEqual(self.found["self-assign"], [21, 22])

    def test_messages_name_the_call_and_the_function(self):
        hits = dict(patterns.scan(parse_tree(fixtures.HOT_PATH))["lookup-in-hot-path"])
        self.assertIn("$path inside _process()", hits[12])
        self.assertIn("get_node_or_null() inside _physics_process()", hits[41])

    def test_runner_scripts_are_exempt_from_the_hot_path_rules(self):
        self.assertEqual(lines(fixtures.RUNNER), {rule: [] for rule in (*patterns.COUNTED, *patterns.ADVISORY)})

    def test_a_runner_keeps_its_exemption_past_an_inner_class_with_another_base(self):
        source = """\
extends SceneTree

class Helper extends RefCounted:
	func _process(_delta: float) -> void:
		var root := get_node("Main")
"""
        self.assertEqual(lines(source)["lookup-in-hot-path"], [])

    def test_a_class_name_runner_is_exempt(self):
        source = """\
class_name Runner extends SceneTree


func _process(_delta: float) -> void:
	var root := get_node("Main")
"""
        self.assertEqual(lines(source)["lookup-in-hot-path"], [])

    def test_an_inner_runner_does_not_exempt_the_script(self):
        source = """\
extends Node

class Runner extends MainLoop:
	pass


func _process(_delta: float) -> void:
	var root := get_node("Main")
"""
        self.assertEqual(lines(source)["lookup-in-hot-path"], [8])


class PatternCheckTest(unittest.TestCase):
    def setUp(self):
        self.repo = Repo({"hot.gd": fixtures.HOT_PATH})
        self.addCleanup(self.repo.cleanup)
        self.config = self.repo.config()

    def test_counted_rules_fail_and_allocations_only_warn(self):
        status, output = captured(check, self.config)
        self.assertEqual(status, 1)
        self.assertEqual(output.count("(lookup-in-hot-path)"), 7)
        self.assertIn("hot.gd:18: warning: ", output)

    def test_counted_rules_are_baselined_per_file(self):
        captured(update_baseline, self.config, init=True)
        self.assertEqual(load(self.config.baseline_path)["lookup-in-hot-path"], {"hot.gd": 7})
        self.assertEqual(captured(check, self.config)[0], 0)
        self.repo.write("hot.gd", fixtures.HOT_PATH + "\n\nfunc _input(_e: InputEvent) -> void:\n\tvar m := $M\n")
        status, output = captured(check, self.config)
        self.assertEqual(status, 1)
        self.assertIn("the file has 8 hot-path lookups, its baseline allows 7", output)

    def test_an_unparsable_file_keeps_its_entries(self):
        captured(update_baseline, self.config, init=True)
        self.repo.write("hot.gd", "func broken(:\n")
        _, output = captured(check, self.config)
        self.assertIn("(parse-error)", output)
        self.assertNotIn("no longer exists", output)

    def test_allocation_warnings_cannot_be_errors(self):
        repo = Repo({}, "rules:\n  per-frame-allocation: error\n")
        self.addCleanup(repo.cleanup)
        with self.assertRaisesRegex(ConfigError, "per-frame-allocation is advisory"):
            repo.config()


if __name__ == "__main__":
    unittest.main()
