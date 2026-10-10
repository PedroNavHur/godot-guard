"""Run every rule on one script and collect measured items and direct problems."""

from __future__ import annotations

import re
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

from . import comments, complexity, interface
from .config import Config, matches, walk_scripts


@dataclass
class Problem:
    path: str
    line: int
    message: str
    rule: str
    level: str = "error"

    def __str__(self) -> str:
        return f"{self.path}:{self.line}: {self.message} ({self.rule})"


@dataclass
class Item:
    """One baselined measurement: a value checked against a limit and the baseline."""

    rule: str
    key: str
    path: str
    line: int
    value: int
    label: str
    locations: list[tuple[int, str]] = field(default_factory=list)


@dataclass
class FileResult:
    path: str
    items: list[Item] = field(default_factory=list)
    problems: list[Problem] = field(default_factory=list)
    parsed: bool = True


class Surfaces:
    """Interface surfaces of every script under the roots, for the Resource exclusion.

    A later root's script replaces an earlier root's at the same relative path; unreadable scripts are skipped.
    """

    def __init__(self, *roots: Path):
        self.by_path = {}
        for root in roots:
            for relative in walk_scripts(root, root):
                source = read_script(root / relative)
                if source is not None:
                    self.by_path[relative] = interface.parse(relative, source)
        self.by_class = {s.class_name: s for s in self.by_path.values() if s.class_name}

    def surface(self, relative: str, source: str) -> interface.Surface | None:
        """The script's surface, or None for a Resource script."""
        surface = interface.parse(relative, source)
        self.by_path.setdefault(relative, surface)
        if interface.is_resource(surface, self.by_class, self.by_path):
            return None
        return surface


def read_script(path: Path) -> str | None:
    """The script's text, or None for a dangling symlink, a directory, or an unreadable file."""
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None


def parse_tree(source: str):
    from gdtoolkit.parser import parser

    return parser.parse(source, gather_metadata=True)


def error_line(error: Exception) -> int:
    return max(getattr(error, "line", None) or 1, 1)


def function_metrics(path: str, name: str, function: complexity.Function) -> list[Item]:
    metrics = (("cognitive", function.cognitive), ("cyclomatic", function.cyclomatic))
    return [
        Item(f"{metric}-complexity", f"{path}::{name}", path, function.line, value,
             f"{name} has {metric} complexity {value}")
        for metric, value in metrics if value is not None
    ]


def unique_names(functions: list[complexity.Function]) -> list[str]:
    """Function names, with `#N` on every occurrence of a name repeated in the file."""
    counts = Counter(f.name for f in functions)
    seen: Counter = Counter()
    names = []
    for function in functions:
        seen[function.name] += 1
        suffix = f"#{seen[function.name]}" if counts[function.name] > 1 else ""
        names.append(function.name + suffix)
    return names


def function_items(path: str, functions: list[complexity.Function]) -> list[Item]:
    """Cognitive and cyclomatic items, one key per function."""
    names = unique_names(functions)
    return [item for name, function in zip(names, functions) for item in function_metrics(path, name, function)]


def tree_rules(result: FileResult, source: str) -> None:
    try:
        tree = parse_tree(source)
    except Exception as error:  # lark raises many parse error types
        result.parsed = False
        first = str(error).splitlines()[0] if str(error) else type(error).__name__
        result.problems.append(Problem(result.path, error_line(error), f"cannot parse: {first}", "parse-error"))
        return
    result.items += function_items(result.path, complexity.score(tree))
    for line in complexity.nested_ternaries(tree):
        message = "nested conditional expression; use if/else, match, or a helper"
        result.problems.append(Problem(result.path, line, message, "nested-ternary", "warn"))


def comment_item(config: Config, path: str, source: str) -> Item:
    cap = config.limits["comment-lines"]
    found = comments.long_comments(source, cap)
    locations = [(c.line, f"comment has {c.prose} prose lines (max {cap})") for c in found]
    line = found[0].line if found else 1
    return Item("long-comment", path, path, line, len(found), f"{len(found)} long comments", locations)


def boundary_problems(config: Config, path: str, source: str) -> list[Problem]:
    lines = comments.strip_comments(source).splitlines()
    problems = []
    for entry in config.boundaries:
        if matches(path, entry["paths"]):
            problems += boundary_hits(entry, path, lines)
    return problems


def boundary_hits(entry: dict, path: str, lines: list[str]) -> list[Problem]:
    patterns = [re.compile(p) for p in entry["forbid"]]
    hits = [(number, p.search(code)) for number, code in enumerate(lines, start=1) for p in patterns]
    suffix = f"; {entry['message']}" if entry["message"] else ""
    return [
        Problem(path, number, f"{entry['name']}: '{found.group(0)}' is forbidden here{suffix}", "boundary")
        for number, found in hits if found
    ]


def analyze(config: Config, path: str, surfaces: Surfaces) -> FileResult:
    source = (config.root / path).read_text(encoding="utf-8", errors="replace")
    result = FileResult(path)
    tree_rules(result, source)
    lines = comments.code_lines(source)
    result.items.append(Item("file-lines", path, path, 1, lines, f"{lines} code lines"))
    result.items.append(comment_item(config, path, source))
    surface = surfaces.surface(path, source)
    if surface is not None:
        detail = f" ({surface.describe()})" if surface.total else ""
        result.items.append(Item("interface-cap", path, path, 1, surface.total, f"{surface.total} inputs{detail}"))
    result.problems += boundary_problems(config, path, source)
    return result
