"""`godot-guard check` and `godot-guard baseline`."""

from __future__ import annotations

from pathlib import Path

from . import baseline
from .config import BASELINE_NAME, Config
from .rules import FileResult, Problem, Surfaces, analyze


def select(config: Config, files: list[str]) -> list[str]:
    """Repo-relative scanned scripts among `files` (paths relative to the working directory)."""
    chosen = set()
    for name in files:
        path = Path(name).resolve()
        try:
            relative = path.relative_to(config.root).as_posix()
        except ValueError:
            continue
        if path.is_file() and config.is_scanned(relative):
            chosen.add(relative)
    return sorted(chosen)


def measure(config: Config, paths: list[str]) -> list[FileResult]:
    surfaces = Surfaces(config.root)
    return [analyze(config, path, surfaces) for path in paths]


def problems_of(config: Config, results: list[FileResult], subset: set[str] | None) -> list[Problem]:
    judge = baseline.judge(config)
    problems = []
    for result in results:
        problems += [p for p in result.problems if config.level(p.rule) != "off"]
        for item in result.items:
            if config.level(item.rule) != "off":
                problems += judge.item(item)
    problems += judge.missing(results, subset)
    return problems


def check(config: Config, files: list[str] | None = None) -> int:
    """Run the rules on `files` (every scanned script when None); 1 on any error."""
    paths = config.scanned_files() if files is None else select(config, files)
    subset = None if files is None else set(paths)
    problems = problems_of(config, measure(config, paths), subset)
    for problem in sorted(problems, key=lambda p: (p.path, p.line, p.rule)):
        prefix = "warning: " if problem.level == "warn" else ""
        print(f"{problem.path}:{problem.line}: {prefix}{problem.message} ({problem.rule})")
    errors = sum(1 for p in problems if p.level == "error")
    warnings = len(problems) - errors
    print(f"godot-guard check: {plural(errors, 'error')}, {plural(warnings, 'warning')}, {plural(len(paths), 'file')}")
    return int(errors > 0)


def plural(count: int, noun: str) -> str:
    return f"{count} {noun}" if count == 1 else f"{count} {noun}s"


def update_baseline(config: Config, init: bool = False, force: bool = False) -> int:
    path = config.baseline_path
    if init and path.exists() and not force:
        print(f"{BASELINE_NAME} already exists; pass --force to overwrite it")
        return 1
    if not init and not path.exists():
        print(f"no {BASELINE_NAME}; nothing to tighten (create one with `godot-guard baseline --init`)")
        return 0
    results = measure(config, config.scanned_files())
    if init:
        data = baseline.violations(config, results)
        baseline.write(path, data)
        counts = ", ".join(f"{rule} {len(data[rule])}" for rule in baseline.BASELINED)
        print(f"wrote {BASELINE_NAME}: {counts}")
        return 0
    data, changes = baseline.tighten(config, baseline.load(path), results)
    for change in changes:
        print(change)
    if changes:
        baseline.write(path, data)
    print(f"{BASELINE_NAME}: {plural(len(changes), 'change')}")
    return 0
