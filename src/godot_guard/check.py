"""`godot-guard check` and `godot-guard baseline`."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from . import baseline
from .config import BASELINE_NAME, Config, hidden, matches, walk_scripts
from .rules import FileResult, Problem, Surfaces, analyze
from .tools import Echo, write_line

JSON_VERSION = 1


def select(config: Config, files: list[str]) -> list[str]:
    """Repo-relative scanned scripts among `files` (paths relative to the working directory)."""
    chosen = set()
    for name in files:
        chosen.update(expand(config, root_relative(config, name)))
    return sorted(chosen)


def root_relative(config: Config, name: str) -> str | None:
    """`name` relative to the root, without resolving symlinks below the root; None outside it."""
    path = Path(os.path.abspath(name))
    for ancestor in (*reversed(path.parents), path):
        if ancestor.resolve() == config.root:
            return path.relative_to(ancestor).as_posix()
    return None


def expand(config: Config, relative: str | None) -> list[str]:
    """Scanned scripts at `relative`: the file itself, or every one below a directory."""
    if relative is None or not (config.root / relative).exists():
        return []
    path = config.root / relative
    reason = skip_reason(config, relative, path.is_dir())
    if reason:
        print(f"godot-guard: skipping {relative}: {reason}", file=sys.stderr)
        return []
    return [relative for relative in walk_scripts(config.root, path) if config.is_scanned(relative)]


def skip_reason(config: Config, relative: str, is_dir: bool) -> str | None:
    """Why an argument selects nothing: it or a directory above it is hidden or excluded."""
    if relative == ".":
        return None
    probe = f"{relative}/" if is_dir else relative
    if hidden(probe):
        return "hidden directories are not scanned"
    if matches(probe, config.exclude):
        return "it matches exclude"
    return None


def measure(config: Config, paths: list[str], surfaces: Surfaces | None = None) -> list[FileResult]:
    surfaces = surfaces or Surfaces(config.root)
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


def check(config: Config, files: list[str] | None = None, surfaces: Surfaces | None = None,
          as_json: bool = False, echo: Echo | None = None) -> int:
    """Run the rules on `files` (every scanned script when None); 1 on any error.

    `surfaces` is the Resource map, read from the root's scripts when None. Output goes to `echo`, or stdout.
    """
    paths = config.scanned_files() if files is None else select(config, files)
    subset = None if files is None else set(paths)
    problems = problems_of(config, measure(config, paths, surfaces), subset)
    problems.sort(key=lambda p: (p.path, p.line, p.rule))
    if as_json:
        write_line(echo, json.dumps(report(config, paths, problems), indent=2))
    else:
        print_text(paths, problems, echo)
    return int(any(p.level == "error" for p in problems))


def print_text(paths: list[str], problems: list[Problem], echo: Echo | None = None) -> None:
    for problem in problems:
        prefix = "warning: " if problem.level == "warn" else ""
        write_line(echo, f"{problem.path}:{problem.line}: {prefix}{problem.message} ({problem.rule})")
    errors = sum(1 for p in problems if p.level == "error")
    warnings = len(problems) - errors
    write_line(echo, f"godot-guard check: {plural(errors, 'error')}, {plural(warnings, 'warning')}, {plural(len(paths), 'file')}")


def report(config: Config, paths: list[str], problems: list[Problem]) -> dict:
    """`check --json`: the effective settings, the checked files, and every problem `check` prints."""
    return {
        "version": JSON_VERSION,
        "root": str(config.root),
        "limits": dict(config.limits),
        "levels": dict(config.rules),
        "files": paths,
        "problems": [
            {"path": p.path, "line": p.line, "rule": p.rule, "level": p.level, "message": p.message}
            for p in problems
        ],
    }


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
