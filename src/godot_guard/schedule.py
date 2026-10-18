"""Gate steps: what each one is, how they run in order, and how a phase runs them on a worker pool."""

from __future__ import annotations

import concurrent.futures
import os
import signal
import sys
import tempfile
import threading
import time
from contextlib import contextmanager
from dataclasses import dataclass
from typing import Callable, Iterator

from .tools import CHILDREN, Echo, ToolError, write_stdout

# Phases run in this order; steps within one phase may run concurrently.
PREPARE = "prepare"  # format, lint, check, and the gate.steps commands
IMPORT = "import"  # Godot's import, alone
TEST = "test"  # the analyzer, every test process, and smoke
PHASES = (PREPARE, IMPORT, TEST)


@dataclass(frozen=True)
class Step:
    name: str
    phase: str
    # action(echo, env): echo None writes to the terminal; env is None or a Godot process's environment.
    action: Callable[[Echo | None, dict | None], int]
    godot: bool = False


@dataclass(frozen=True)
class Result:
    name: str
    status: int
    output: str
    elapsed: float


def run_steps(steps: list[Step], jobs: int) -> int:
    """With one job, run the steps in order, streamed. With more, run each phase on a pool of `jobs` workers."""
    started = time.monotonic()
    with interruptible():
        failed = run_serial(steps) if jobs == 1 else run_phases(steps, jobs)
    if failed:
        return 1
    print(f"gate passed in {time.monotonic() - started:.1f}s")
    return 0


def run_serial(steps: list[Step]) -> list[str]:
    for step in steps:
        print(f"== {step.name}", flush=True)
        began = time.monotonic()
        status = run_step(step, None, None)
        elapsed = time.monotonic() - began
        if status:
            print(f"== {step.name} failed after {elapsed:.1f}s", flush=True)
            return [step.name]
        print(f"== {step.name} passed in {elapsed:.1f}s", flush=True)
    return []


def run_phases(steps: list[Step], jobs: int) -> list[str]:
    for phase in PHASES:
        group = [step for step in steps if step.phase == phase]
        failed = run_concurrent(group, jobs) if group else []
        if failed:
            print(f"== failed: {', '.join(failed)}", flush=True)
            return failed
    return []


def run_concurrent(steps: list[Step], jobs: int) -> list[str]:
    """At most `jobs` steps at once. After the first failure no new step starts; running ones finish and print."""
    stop = threading.Event()
    pool = concurrent.futures.ThreadPoolExecutor(max_workers=min(jobs, len(steps)))
    failed: list[str] = []
    try:
        futures = [pool.submit(run_captured, step, stop) for step in steps]
        for future in concurrent.futures.as_completed(futures):
            result = future.result()
            if result is None:
                continue
            print_block(result)
            if result.status:
                failed.append(result.name)
    finally:
        pool.shutdown(wait=True, cancel_futures=True)
    return failed


def run_captured(step: Step, stop: threading.Event) -> Result | None:
    """The step with its output captured, or None when a failure has already stopped the phase."""
    if stop.is_set():
        return None
    lines: list[str] = []
    began = time.monotonic()
    with isolation(step) as env:
        status = run_step(step, lines.append, env)
    if status:
        stop.set()
    return Result(step.name, status, "".join(lines), time.monotonic() - began)


def print_block(result: Result) -> None:
    """One step's output, framed by its name and how it ended."""
    output = result.output
    if output and not output.endswith("\n"):
        output += "\n"
    print(f"== {result.name}", flush=True)
    write_stdout(output)
    if result.status:
        print(f"== {result.name} failed after {result.elapsed:.1f}s", flush=True)
    else:
        print(f"== {result.name} passed in {result.elapsed:.1f}s", flush=True)


def run_step(step: Step, echo: Echo | None, env: dict | None) -> int:
    """The step's status. A missing tool fails it, with the reason written to its output."""
    try:
        return step.action(echo, env)
    except ToolError as error:
        (echo or write_stdout)(f"godot-guard: {error}\n")
        return 127


@contextmanager
def isolation(step: Step) -> Iterator[dict | None]:
    """For a Godot step, an environment with a fresh user-data dir, removed afterwards. Other steps get none."""
    if not step.godot:
        yield None
        return
    with tempfile.TemporaryDirectory(prefix="godot-guard-") as temp:
        env = dict(os.environ, XDG_DATA_HOME=temp)
        if sys.platform == "darwin":
            # Godot on macOS ignores XDG_DATA_HOME; HOME moves its user:// directory.
            env["HOME"] = temp
        yield env


@contextmanager
def interruptible() -> Iterator[None]:
    """On SIGINT or SIGTERM, kill the children this gate started, then exit with 128 plus the signal number."""
    previous = install_stop_handlers() if threading.current_thread() is threading.main_thread() else {}
    try:
        yield
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)
        CHILDREN.resume()


def install_stop_handlers() -> dict[int, object]:
    """Install the stop handler; returns the handlers it replaced, keyed by signal number."""

    def stop(signum: int, frame: object) -> None:
        CHILDREN.stop()
        raise SystemExit(128 + signum)

    return {number: signal.signal(number, stop) for number in (signal.SIGINT, signal.SIGTERM)}
