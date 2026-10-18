from __future__ import annotations

import os
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

import godot_guard
from godot_guard import schedule, tools
from godot_guard.schedule import IMPORT, PREPARE, TEST, Step, interruptible, isolation, run_concurrent, run_steps
from godot_guard.tools import CHILDREN, ToolError, write_line

from .helpers import captured


def fake(name, phase=PREPARE, status=0, log=None, lines=(), action=None):
    """A step that records its name in `log`, writes `lines` through its echo, and returns `status`."""

    def run(echo, env):
        if action is not None:
            action(env)
        if log is not None:
            log.append(name)
        for line in lines:
            write_line(echo, line)
        return status

    return Step(name, phase, run)


class PhaseOrderTest(unittest.TestCase):
    def test_phases_run_in_order_whatever_the_config_order(self):
        log = []
        steps = [fake("t", TEST, log=log), fake("p", PREPARE, log=log), fake("i", IMPORT, log=log)]
        status, _ = captured(run_steps, steps, 2)
        self.assertEqual(status, 0)
        self.assertEqual(log, ["p", "i", "t"])

    def test_a_failed_phase_skips_the_later_ones(self):
        log = []
        steps = [fake("bad", PREPARE, status=1, log=log), fake("i", IMPORT, log=log), fake("t", TEST, log=log)]
        status, output = captured(run_steps, steps, 2)
        self.assertEqual(status, 1)
        self.assertEqual(log, ["bad"])
        self.assertIn("== failed: bad\n", output)

    def test_a_single_job_runs_in_config_order_and_stops_at_the_first_failure(self):
        log = []
        steps = [fake("a", log=log), fake("bad", status=1, log=log), fake("c", log=log)]
        status, output = captured(run_steps, steps, 1)
        self.assertEqual(status, 1)
        self.assertEqual(log, ["a", "bad"])
        self.assertIn("== bad failed after", output)


class ConcurrencyTest(unittest.TestCase):
    def test_a_phase_runs_at_most_jobs_steps_at_once(self):
        # A barrier of two only passes when two steps overlap; four steps on two workers fill it twice.
        barrier = threading.Barrier(2, timeout=5)
        lock = threading.Lock()
        active, peak = [0], [0]

        def work(env):
            with lock:
                active[0] += 1
                peak[0] = max(peak[0], active[0])
            barrier.wait()
            with lock:
                active[0] -= 1

        steps = [fake(f"s{i}", action=work) for i in range(4)]
        status, _ = captured(run_steps, steps, 2)
        self.assertEqual(status, 0)
        self.assertEqual(peak[0], 2)

    def test_each_block_is_framed_and_holds_only_its_own_output(self):
        steps = [fake("a", lines=["a1", "a2"]), fake("b", lines=["b1"])]
        status, output = captured(run_steps, steps, 2)
        self.assertEqual(status, 0)
        for name, lines in (("a", ["a1", "a2"]), ("b", ["b1"])):
            pattern = rf"== {name}\n" + "".join(f"{line}\n" for line in lines) + rf"== {name} passed in \d+\.\ds\n"
            self.assertRegex(output, pattern)

    def test_a_failed_block_is_framed_as_failed(self):
        status, output = captured(run_steps, [fake("bad", status=3, lines=["oops"])], 2)
        self.assertEqual(status, 1)
        self.assertRegex(output, r"== bad\noops\n== bad failed after \d+\.\ds\n== failed: bad\n")


class FailFastTest(unittest.TestCase):
    def test_no_new_step_starts_after_a_failure(self):
        log = []
        steps = [fake("bad", status=1, log=log)] + [fake(f"later{i}", log=log) for i in range(3)]
        failed, _ = captured(run_concurrent, steps, 1)
        self.assertEqual(failed, ["bad"])
        self.assertEqual(log, ["bad"])

    def test_running_steps_finish_and_print_after_a_failure(self):
        # `bad` fails only once `slow` is running, so `slow` is in flight when the failure lands.
        started = threading.Event()

        def slow(env):
            started.set()
            time.sleep(0.2)

        steps = [
            fake("slow", PREPARE, lines=["done"], action=slow),
            fake("bad", PREPARE, status=1, action=lambda env: started.wait(5)),
        ]
        status, output = captured(run_steps, steps, 2)
        self.assertEqual(status, 1)
        self.assertRegex(output, r"== slow\ndone\n== slow passed in")


class IsolationTest(unittest.TestCase):
    def godot_step(self):
        return Step("tests", TEST, lambda echo, env: 0, godot=True)

    def test_godot_steps_get_a_fresh_data_dir_that_is_removed_afterwards(self):
        with mock.patch.dict(os.environ, {"XDG_DATA_HOME": "/inherited"}):
            with isolation(self.godot_step()) as env:
                data = Path(env["XDG_DATA_HOME"])
                self.assertTrue(data.is_dir())
                self.assertNotEqual(str(data), "/inherited")
        self.assertFalse(data.exists())

    def test_macos_points_home_at_the_data_dir_too(self):
        with mock.patch.object(schedule.sys, "platform", "darwin"):
            with isolation(self.godot_step()) as env:
                self.assertEqual(env["HOME"], env["XDG_DATA_HOME"])

    def test_other_platforms_keep_home(self):
        with mock.patch.object(schedule.sys, "platform", "linux"):
            with isolation(self.godot_step()) as env:
                self.assertEqual(env["HOME"], os.environ["HOME"])

    def test_other_steps_get_no_environment(self):
        with isolation(fake("lint")) as env:
            self.assertIsNone(env)

    def test_godot_import_gets_no_environment_even_on_a_pool(self):
        seen = []
        step = Step("import", IMPORT, lambda echo, env: seen.append(env) or 0, godot=True)
        status, _ = captured(run_steps, [step], 2)
        self.assertEqual(status, 0)
        self.assertEqual(seen, [None])

    def test_godot_steps_in_a_phase_get_their_own_dirs(self):
        seen = []

        def record(env):
            seen.append(env["XDG_DATA_HOME"])

        steps = [
            Step("one", TEST, lambda echo, env: record(env) or 0, godot=True),
            Step("two", TEST, lambda echo, env: record(env) or 0, godot=True),
        ]
        status, _ = captured(run_steps, steps, 2)
        self.assertEqual(status, 0)
        self.assertEqual(len(set(seen)), 2)


class InterruptTest(unittest.TestCase):
    def test_stop_kills_live_children_and_refuses_new_ones(self):
        self.addCleanup(CHILDREN.resume)
        process = CHILDREN.start(["sleep", "30"], None, None, pipe=False)
        CHILDREN.stop(signal.SIGTERM)
        self.assertEqual(process.wait(timeout=5), -signal.SIGKILL)
        CHILDREN.finish(process)
        with self.assertRaises(ToolError):
            CHILDREN.start(["true"], None, None, pipe=False)

    def test_a_signal_inside_a_spawn_kills_the_child_and_exits(self):
        # The signal lands on this thread after Popen returns and before the child is registered.
        spawned = []
        real_popen = subprocess.Popen

        def popen_then_signal(*args, **kwargs):
            process = real_popen(*args, **kwargs)
            self.addCleanup(process.kill)
            spawned.append(process)
            signal.raise_signal(signal.SIGTERM)
            return process

        with mock.patch.object(tools.subprocess, "Popen", popen_then_signal):
            with interruptible(), self.assertRaises(SystemExit) as caught:
                CHILDREN.start(["sleep", "30"], None, None, pipe=False)
        self.assertEqual(caught.exception.code, 128 + signal.SIGTERM)
        self.assertEqual(spawned[0].wait(timeout=5), -signal.SIGKILL)
        with self.assertRaises(ProcessLookupError):
            os.kill(spawned[0].pid, 0)

    def test_a_stop_from_another_thread_waits_for_a_spawn_and_kills_its_child(self):
        self.addCleanup(CHILDREN.resume)
        spawned = []
        inside, release = threading.Event(), threading.Event()
        real_popen = subprocess.Popen

        def popen_and_hold(*args, **kwargs):
            process = real_popen(*args, **kwargs)
            self.addCleanup(process.kill)
            spawned.append(process)
            inside.set()
            release.wait(5)
            return process

        with mock.patch.object(tools.subprocess, "Popen", popen_and_hold):
            spawner = threading.Thread(target=CHILDREN.start, args=(["sleep", "30"], None, None, False))
            spawner.start()
            self.assertTrue(inside.wait(5))
            stopper = threading.Thread(target=CHILDREN.stop, args=(signal.SIGTERM,))
            stopper.start()
            release.set()
            spawner.join(5)
            stopper.join(5)
        self.assertEqual(spawned[0].wait(timeout=5), -signal.SIGKILL)

    def test_sigterm_kills_the_gate_children_and_exits_with_143(self):
        with tempfile.TemporaryDirectory() as temp:
            pidfile = Path(temp) / "pid"
            runner = subprocess.Popen(
                [sys.executable, "-c", RUNNER, str(pidfile)],
                env=dict(os.environ, PYTHONPATH=str(Path(godot_guard.__file__).parents[1])),
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            self.addCleanup(runner.kill)
            pid = wait_for_pid(pidfile)
            runner.send_signal(signal.SIGTERM)
            self.assertEqual(runner.wait(timeout=20), 128 + signal.SIGTERM)
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)


# A gate whose one step runs `sleep 30` and records its pid; the runner is killed by SIGTERM mid-step.
RUNNER = """
import sys
from godot_guard.schedule import PREPARE, Step, run_steps
from godot_guard.tools import run_process

pidfile = sys.argv[1]
hang = Step("hang", PREPARE, lambda echo, env: run_process(
    ["sh", "-c", 'echo $$ > "$1"; exec sleep 30', "sh", pidfile], None, None, echo))
sys.exit(run_steps([hang], 2))
"""


def wait_for_pid(pidfile: Path) -> int:
    deadline = time.monotonic() + 10
    while not pidfile.exists() or not pidfile.read_text().strip():
        if time.monotonic() > deadline:
            raise AssertionError("the runner never started its child")
        time.sleep(0.05)
    return int(pidfile.read_text())


if __name__ == "__main__":
    unittest.main()
