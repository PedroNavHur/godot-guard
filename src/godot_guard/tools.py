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
        self._lock = threading.Lock()
        self._live: set[subprocess.Popen] = set()

    def start(self, command: list[str], cwd: Path | None, env: dict | None, pipe: bool) -> subprocess.Popen:
        """Start `command` in its own session, so a kill reaches everything it started."""
        with self._lock:
            process = subprocess.Popen(
                command, cwd=cwd, env=env, start_new_session=True,
                stdout=subprocess.PIPE if pipe else None,
                stderr=subprocess.STDOUT if pipe else None,
                text=True, errors="replace",
            )
            self._live.add(process)
        return process

    def finish(self, process: subprocess.Popen) -> None:
        """Forget `process`; if it is still running (its reader raised), kill it and reap it."""
        with self._lock:
            self._live.discard(process)
        if process.poll() is None:
            kill_session(process)
            process.wait()
        if process.stdout:
            process.stdout.close()


CHILDREN = Children()


def kill_session(process: subprocess.Popen) -> None:
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
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
