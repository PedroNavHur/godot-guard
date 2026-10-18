from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from godot_guard import jobs

PROC_LINE = "0::{}\n"


class AutoJobsTest(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.proc = self.root / "proc-cgroup"
        usable = mock.patch.object(jobs, "usable_cpus", return_value=12)
        usable.start()
        self.addCleanup(usable.stop)

    def cgroup(self, path: str, limits: dict[str, str]) -> None:
        for directory, text in limits.items():
            (self.root / directory).mkdir(parents=True, exist_ok=True)
            (self.root / directory / "cpu.max").write_text(text)
        self.proc.write_text(PROC_LINE.format(path))

    def test_quota_rounds_up_to_whole_cpus(self):
        self.cgroup("/a/b", {"a/b": "900000 100000\n"})
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 9)
        self.cgroup("/a/b", {"a/b": "250000 100000\n"})
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 3)
        self.cgroup("/a/b", {"a/b": "1000 100000\n"})
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 1)

    def test_tightest_ancestor_wins(self):
        self.cgroup("/a/b", {"a": "150000 100000\n", "a/b": "max 100000\n"})
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 2)
        self.cgroup("/a/b", {"": "max 100000\n", "a": "1100000 100000\n"})
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 11)

    def test_limit_above_usable_cpus_is_ignored(self):
        self.cgroup("/a", {"a": "4000000 100000\n"})
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 12)

    def test_unlimited_or_unknown_cgroups_use_every_cpu(self):
        self.cgroup("/a/b", {"a/b": "max 100000\n"})
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 12)
        self.proc.write_text("1:cpu,cpuacct:/a\n")
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 12)
        self.assertEqual(jobs.auto_jobs(self.root, self.root / "missing"), 12)

    def test_never_below_one(self):
        self.cgroup("/", {"": "0 100000\n"})
        self.assertEqual(jobs.auto_jobs(self.root, self.proc), 1)


if __name__ == "__main__":
    unittest.main()
