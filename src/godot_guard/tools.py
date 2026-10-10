"""Locate Godot and gdtoolkit, and run Godot with its diagnostics treated as failures."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

MAC_GODOT = "/Applications/Godot.app/Contents/MacOS/Godot"


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


def run_godot(args: list[str], cwd: Path | None = None) -> int:
    """Stream Godot's output; fail on a non-zero exit or any diagnostic line."""
    command = [godot(), *args]
    failed = False
    with subprocess.Popen(command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace") as process:
        for line in process.stdout:
            print(line, end="", flush=True)
            failed = failed or is_diagnostic(line)
        status = process.wait()
    if status:
        return status
    return int(failed)
