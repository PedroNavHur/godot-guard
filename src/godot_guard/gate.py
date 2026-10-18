"""`godot-guard gate`: the fast pre-commit gate and the full pre-push and CI gate."""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

from .check import check, select
from .config import Config, TestScript
from .rules import Surfaces
from .snapshot import indexed, snapshot, staged
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
    steps += test_steps(gate["tests"])
    if gate["smoke"]:
        steps.append(("smoke", list(gate["smoke"])))
    return steps


def test_steps(tests: list[TestScript]) -> list[tuple[str, list[str]]]:
    return [step for test in tests for step in script_steps(test)]


def script_steps(test: TestScript) -> list[tuple[str, list[str]]]:
    """One step for a script; a sharded script gets one per shard, each passed `-- --shard=I/N`."""
    if test.shards == 1:
        return [(f"tests {test.script}", ["--script", test.script])]
    return [
        (f"tests {test.script} [{shard}/{test.shards}]", ["--script", test.script, "--", f"--shard={shard}/{test.shards}"])
        for shard in range(1, test.shards + 1)
    ]


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
    steps.append(("check", lambda: check(sources, absolute(sources, chosen), resource_map(config, sources, chosen))))
    if fast:
        return steps
    for command in gate["steps"]:
        steps.append((command, lambda c=command: subprocess.run(c, shell=True, cwd=config.root).returncode))
    root = str(config.root)
    for name, args in godot_steps(config):
        steps.append((name, lambda a=args: run_godot(["--headless", "--path", root, *a])))
    return steps


def resource_map(config: Config, sources: Config, chosen: list[str] | None) -> Surfaces | None:
    """For a snapshot: the index's other scripts as in the working tree, overlaid with the snapshot's."""
    if sources is config:
        return None
    surfaces = Surfaces(config.root, indexed(config, chosen))
    surfaces.overlay(sources.root, chosen)
    return surfaces


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
    chosen, links = staged(config)
    for link in links:
        print(f"{link}: symlinked scripts are not supported; commit the real file")
    if links:
        return 1
    if not chosen:
        return run_gate(config, chosen, fast)
    with snapshot(config, chosen) as sources:
        return run_gate(config, chosen, fast, sources)
