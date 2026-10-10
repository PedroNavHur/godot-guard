"""`godot-guard gate`: the fast pre-commit gate and the full pre-push and CI gate."""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

from .check import check, select
from .config import Config
from .rules import Surfaces
from .snapshot import snapshot, staged
from .tools import ToolError, gdtoolkit, run_godot

ANALYZER = Path(__file__).parent / "data" / "analyze_scripts.gd"


def gdtool(name: str, args: list[str], files: list[str], config: Config) -> int:
    if not files:
        return 0
    return subprocess.run([gdtoolkit(name), *args, *files], cwd=config.root).returncode


def res_path(path: str) -> str:
    if path == ".":
        return "res://"
    return "res://" + path


def godot_steps(config: Config) -> list[tuple[str, list[str]]]:
    """Godot steps in order, as (name, args after --headless --path ROOT)."""
    gate, steps = config.gate, []
    if gate["import"]:
        steps.append(("import", ["--editor", "--quit"]))
    if gate["analyzer"]:
        roots = [arg for path in config.paths for arg in ("--root", res_path(path))]
        excludes = [arg for pattern in config.exclude for arg in ("--exclude", pattern)]
        steps.append(("analyzer", ["--script", str(ANALYZER), "--", *roots, *excludes]))
    steps += [(f"tests {script}", ["--script", script]) for script in gate["tests"]]
    if gate["smoke"]:
        steps.append(("smoke", list(gate["smoke"])))
    return steps


def steps_for(
    config: Config, chosen: list[str] | None, fast: bool, sources: Config | None = None
) -> list[tuple[str, object]]:
    """`chosen` are the selected scripts, or None for every scanned one; `sources` roots the copy they are read from."""
    sources = sources or config
    files = sources.scanned_files() if chosen is None else chosen
    gate, steps = config.gate, []
    if gate["format"]:
        steps.append(("format", lambda: gdtool("gdformat", ["--check"], files, sources)))
    if gate["lint"]:
        steps.append(("lint", lambda: gdtool("gdlint", [], files, sources)))
    steps.append(("check", lambda: check(sources, absolute(sources, chosen), resource_map(config, sources))))
    if fast:
        return steps
    for command in gate["steps"]:
        steps.append((command, lambda c=command: subprocess.run(c, shell=True, cwd=config.root).returncode))
    root = str(config.root)
    for name, args in godot_steps(config):
        steps.append((name, lambda a=args: run_godot(["--headless", "--path", root, *a])))
    return steps


def resource_map(config: Config, sources: Config) -> Surfaces | None:
    """The working tree's scripts overlaid with the snapshot's, when the scripts come from a snapshot."""
    if sources is config:
        return None
    return Surfaces(config.root, sources.root)


def run_steps(steps: list[tuple[str, object]]) -> int:
    started = time.monotonic()
    for name, action in steps:
        print(f"== {name}", flush=True)
        began = time.monotonic()
        try:
            status = action()
        except ToolError as error:
            print(f"godot-guard: {error}")
            status = 127
        elapsed = time.monotonic() - began
        if status:
            print(f"== {name} failed after {elapsed:.1f}s", flush=True)
            return 1
        print(f"== {name} passed in {elapsed:.1f}s", flush=True)
    print(f"gate passed in {time.monotonic() - started:.1f}s")
    return 0


def absolute(config: Config, chosen: list[str] | None) -> list[str] | None:
    if chosen is None:
        return None
    return [str(config.root / path) for path in chosen]


def run_gate(config: Config, chosen: list[str] | None, fast: bool, sources: Config | None = None) -> int:
    if chosen == [] and fast:
        print("godot-guard gate: no GDScript files selected")
        return 0
    return run_steps(steps_for(config, chosen, fast, sources))


def gate(config: Config, fast: bool, use_staged: bool, files: list[str]) -> int:
    """`--staged` checks the index's content of the staged scripts, not the working tree's."""
    if not use_staged:
        return run_gate(config, select(config, files) if files else None, fast)
    chosen = staged(config)
    if not chosen:
        return run_gate(config, chosen, fast)
    with snapshot(config, chosen) as sources:
        return run_gate(config, chosen, fast, sources)
