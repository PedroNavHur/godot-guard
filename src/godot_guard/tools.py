"""Locate Godot and gdtoolkit, and run Godot with its diagnostics treated as failures."""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import sys
import threading
from collections.abc import Callable
from pathlib import Path

MAC_GODOT = "/Applications/Godot.app/Contents/MacOS/Godot"

# Output text as a child writes it, each piece ending in a newline.
Echo = Callable[[str], None]


class ToolError(Exception):
    pass


def godot() -> str:
    """$GODOT, then godot or godot4 on PATH, then the macOS app bundle."""
    if os.environ.get("GODOT"):
        return os.environ["GODOT"]
    for name in ("godot", "godot4"):
        found = shutil.which(name)
        if found:
            return found
    if os.access(MAC_GODOT, os.X_OK):
        return MAC_GODOT
    raise ToolError("Godot 4 not found: set $GODOT or put godot on PATH")


def gdtoolkit(name: str) -> str:
    """gdformat or gdlint next to this interpreter, then on PATH."""
    beside = Path(sys.executable).parent / name
    if os.access(beside, os.X_OK):
        return str(beside)
    found = shutil.which(name)
    if found:
        return found
    raise ToolError(f"{name} not found: install gdtoolkit next to godot-guard")


def is_diagnostic(line: str) -> bool:
    return "SCRIPT ERROR:" in line or line.startswith(("ERROR:", "WARNING:"))


def write_stdout(text: str) -> None:
    print(text, end="", flush=True)


def write_line(echo: Echo | None, line: str) -> None:
    (echo or write_stdout)(line + "\n")


class Children:
    """Live child processes, so that a stopped gate leaves none running."""

    def __init__(self) -> None:
        self._lock = threading.RLock()  # a signal handler may run on the thread that holds it
        self._live: set[subprocess.Popen] = set()
        self._stopped = False
        self._spawner: int | None = None  # the thread inside a spawn, if any
        self._deferred: list[int] = []  # signals that arrived inside a spawn on that thread

    def start(self, command: list[str], cwd: Path | None, env: dict | None, pipe: bool) -> subprocess.Popen:
        """Start `command` in its own session, so a kill reaches everything it started.

        The spawn holds the lock, so a stop from another thread waits until the child is registered and
        then kills it. A signal on this thread can run its handler mid-spawn; `stop` defers that signal,
        and it is raised again once the child is registered, so the handler kills the child and exits.
        """
        with self._lock:
            if self._stopped:
                raise ToolError("interrupted; not starting another process")
            self._spawner = threading.get_ident()
            process = None
            try:
                process = subprocess.Popen(
                    command, cwd=cwd, env=env, start_new_session=True,
                    stdout=subprocess.PIPE if pipe else None,
                    stderr=subprocess.STDOUT if pipe else None,
                    text=True, errors="replace",
                )
                self._live.add(process)
            finally:
                self._spawner = None
                self._raise_deferred(process)
        return process

    def stop(self, signum: int) -> bool:
        """Refuse new children and kill the live ones; `resume` lifts the refusal.

        Never waits: the stop handler can run on the thread blocked in a child's `wait()`, which holds that
        child's wait lock, so waiting here would deadlock. The children are reaped as the stop's exit unwinds.

        Returns False, deferring `signum`, when this thread is mid-spawn: the spawn raises it once the child is
        registered. Otherwise returns True.
        """
        with self._lock:
            if self._spawner == threading.get_ident():
                self._deferred.append(signum)
                return False
            self._stopped = True
            for process in self._live:
                kill_session(process)
        return True

    def _raise_deferred(self, process: subprocess.Popen | None) -> None:
        """Raise the signals that arrived mid-spawn, now that the spawn is over.

        Their handler only kills, so the exit it starts unwinds past `start` with the child registered. No wait
        lock is held here, so `process` is reaped now rather than left behind.
        """
        signums, self._deferred = self._deferred, []
        try:
            for signum in signums:
                signal.raise_signal(signum)
        except BaseException:
            if process is not None:
                self.finish(process)
            raise

    def resume(self) -> None:
        with self._lock:
            self._stopped = False

    def finish(self, process: subprocess.Popen) -> None:
        """Forget `process`; if it is still running (its reader raised, or a stop unwound), kill it and reap it."""
        with self._lock:
            self._live.discard(process)
        if process.poll() is None:
            kill_session(process)
            process.wait()
        if process.stdout:
            process.stdout.close()

    def reap(self) -> None:
        """Kill and reap any child still registered, for a stop whose unwinding missed it. Runs on the normal stack."""
        with self._lock:
            leftover = list(self._live)
        for process in leftover:
            self.finish(process)


CHILDREN = Children()


def kill_session(process: subprocess.Popen) -> None:
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    except PermissionError:
        # macOS answers EPERM, not ESRCH, once the group's only member is a zombie: it died, and the wait reaps it.
        pass


def run_process(command: list[str], cwd: Path | None, env: dict | None, echo: Echo | None) -> int:
    """Run `command` as a tracked child. With `echo`, its output (stderr merged in) goes to it piece by piece; without, to the terminal."""
    process = CHILDREN.start(command, cwd, env, pipe=echo is not None)
    try:
        if echo is not None:
            for line in process.stdout:
                echo(line)
        return process.wait()
    finally:
        CHILDREN.finish(process)


def run_godot(args: list[str], cwd: Path | None = None, env: dict | None = None, echo: Echo | None = None) -> int:
    """Run Godot; fail on a non-zero exit or any diagnostic line. Output goes to `echo`, or to stdout as it arrives."""
    echo = echo or write_stdout
    diagnostics: list[str] = []

    def watch(line: str) -> None:
        if is_diagnostic(line):
            diagnostics.append(line)
        echo(line)

    status = run_process([godot(), *args], cwd, env, watch)
    return status or int(bool(diagnostics))
