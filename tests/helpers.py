"""Temporary Godot repositories for the tests."""

from __future__ import annotations

import contextlib
import io
import tempfile
from pathlib import Path

from godot_guard import config as config_module


class Repo:
    def __init__(self, files: dict[str, str], config_text: str | None = None):
        self._temp = tempfile.TemporaryDirectory()
        self.root = Path(self._temp.name).resolve()
        self.write("project.godot", "config_version=5\n")
        if config_text is not None:
            self.write(".godot-guard.yml", config_text)
        for name, text in files.items():
            self.write(name, text)

    def write(self, name: str, text: str) -> Path:
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
        return path

    def config(self) -> config_module.Config:
        return config_module.load(self.root)

    def cleanup(self) -> None:
        self._temp.cleanup()


def captured(function, *args, **kwargs) -> tuple[int, str]:
    """Call `function` and return its result with everything it printed."""
    output = io.StringIO()
    with contextlib.redirect_stdout(output):
        status = function(*args, **kwargs)
    return status, output.getvalue()
