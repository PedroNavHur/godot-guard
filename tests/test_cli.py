from __future__ import annotations

import argparse
import unittest
from unittest import mock

from godot_guard.cli import build_parser, jobs_argument, run_gate

from .helpers import Repo


class JobsArgumentTest(unittest.TestCase):
    def test_a_count_or_auto_is_accepted(self):
        self.assertEqual(jobs_argument("1"), 1)
        self.assertEqual(jobs_argument("12"), 12)
        self.assertEqual(jobs_argument("auto"), "auto")

    def test_anything_else_is_rejected(self):
        for value in ("0", "-2", "2.5", "two", "", "auto ", "²"):
            with self.subTest(value=value), self.assertRaises(argparse.ArgumentTypeError):
                jobs_argument(value)

    def test_the_gate_parser_reads_it(self):
        self.assertEqual(build_parser().parse_args(["gate", "--jobs", "3"]).jobs, 3)
        self.assertIsNone(build_parser().parse_args(["gate"]).jobs)


class RunGateTest(unittest.TestCase):
    def setUp(self):
        self.repo = Repo({}, "gate:\n  jobs: 2\n")
        self.addCleanup(self.repo.cleanup)
        self.config = self.repo.config()

    def gate_jobs(self, argv: list[str]) -> object:
        """The `gate.jobs` value the gate was called with for `argv`."""
        with mock.patch("godot_guard.cli.gate", return_value=0) as gate:
            run_gate(self.config, build_parser().parse_args(["gate", *argv]))
        return gate.call_args.args[0].gate["jobs"]

    def test_the_flag_overrides_the_config(self):
        self.assertEqual(self.gate_jobs(["--jobs", "4"]), 4)
        self.assertEqual(self.gate_jobs(["--jobs", "auto"]), "auto")

    def test_without_the_flag_the_config_decides(self):
        self.assertEqual(self.gate_jobs([]), 2)

    def test_the_loaded_config_is_not_changed(self):
        self.gate_jobs(["--jobs", "4"])
        self.assertEqual(self.config.gate["jobs"], 2)


if __name__ == "__main__":
    unittest.main()
