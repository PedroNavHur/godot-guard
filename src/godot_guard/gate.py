"""`godot-guard gate`: the fast pre-commit gate and the full pre-push and CI gate."""

from __future__ import annotations

from pathlib import Path

from .check import check, select
from .config import Config, TestScript
from .jobs import worker_count
from .rules import Surfaces
from .schedule import IMPORT, PREPARE, TEST, Step, run_steps
from .snapshot import indexed, snapshot, staged
from .tools import Echo, gdtoolkit, run_godot, run_process

ANALYZER = Path(__file__).parent / "data" / "analyze_scripts.gd"


def gdtool(name: str, args: list[str], files: list[str], config: Config, echo: Echo | None) -> int:
    if not files:
        return 0
    return run_process([gdtoolkit(name), *args, *files], config.root, None, echo)


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
    """One step for a plain script; a script with `shards`, even 1, gets one per shard, each passed `-- --shard=I/N`."""
    if test.shards is None:
        return [(f"tests {test.script}", ["--script", test.script])]
    return [
        (f"tests {test.script} [{shard}/{test.shards}]", ["--script", test.script, "--", f"--shard={shard}/{test.shards}"])
        for shard in range(1, test.shards + 1)
    ]


def steps_for(
    config: Config, chosen: list[str] | None, fast: bool, sources: Config | None = None
) -> list[Step]:
    """`chosen` are the selected scripts, or None for every scanned one; `sources` roots the copy they are read from."""
    sources = sources or config
    files = sources.scanned_files() if chosen is None else chosen
    gate, steps = config.gate, []
    if gate["format"]:
        steps.append(Step("format", PREPARE, lambda echo, env: gdtool("gdformat", ["--check"], files, sources, echo)))
    if gate["lint"]:
        steps.append(Step("lint", PREPARE, lambda echo, env: gdtool("gdlint", [], files, sources, echo)))
    steps.append(Step("check", PREPARE, lambda echo, env: check(
        sources, absolute(sources, chosen), resource_map(config, sources, chosen), echo=echo)))
    if fast:
        return steps
    for command in gate["steps"]:
        steps.append(Step(command, PREPARE, lambda echo, env, c=command: run_process(["/bin/sh", "-c", c], config.root, None, echo)))
    root = str(config.root)
    for name, args in godot_steps(config):
        phase = IMPORT if name == "import" else TEST
        steps.append(Step(name, phase, lambda echo, env, a=args: run_godot(
            ["--headless", "--path", root, *a], echo=echo, env=env), godot=True))
    return steps


def resource_map(config: Config, sources: Config, chosen: list[str] | None) -> Surfaces | None:
    """For a snapshot: the index's other scripts as in the working tree, overlaid with the snapshot's."""
    if sources is config:
        return None
    surfaces = Surfaces(config.root, indexed(config, chosen))
    surfaces.overlay(sources.root, chosen)
    return surfaces


def absolute(config: Config, chosen: list[str] | None) -> list[str] | None:
    if chosen is None:
        return None
    return [str(config.root / path) for path in chosen]


def warm_parser_cache() -> None:
    """Build gdtoolkit's parsers once, serially, so the parallel PREPARE steps only read its cache.

    gdtoolkit 4.5.0 makes its versioned cache directory with exists() then makedirs() and no exist_ok,
    and writes each parser's pickle in place. Cold starts that overlap (gdformat, gdlint, and check's
    in-process parser) can collide on both. One parse here creates the directory and every pickle first.
    """
    from gdtoolkit.parser import parser

    parser.parse("", gather_metadata=True)
    parser.parse("")
    parser.parse_comments("")


def run_gate(config: Config, chosen: list[str] | None, fast: bool, sources: Config | None = None) -> int:
    if chosen == [] and fast:
        print("godot-guard gate: no GDScript files selected")
        return 0
    jobs = worker_count(config.gate["jobs"])
    if jobs > 1:
        warm_parser_cache()
    return run_steps(steps_for(config, chosen, fast, sources), jobs)


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
