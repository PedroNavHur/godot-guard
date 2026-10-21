"""The ratchet baseline: measured items over their limit, which may only shrink."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path

from .config import BASELINE_NAME, BASELINED, Config
from .patterns import COUNTED
from .rules import FileResult, Item, Problem

LIMIT_OF = {
    "cognitive-complexity": "cognitive",
    "cyclomatic-complexity": "cyclomatic",
    "interface-cap": "inputs",
}
TREE_RULES = ("cognitive-complexity", "cyclomatic-complexity")
# Measured on the parse tree: a file that fails to parse has no item, so its entries are not missing.
PARSED_RULES = (*TREE_RULES, *COUNTED)


def limit(config: Config, rule: str) -> int:
    """Counted rules (long-comment, the pattern rules) count hits, so any is over."""
    if rule not in LIMIT_OF:
        return 0
    return config.limits[LIMIT_OF[rule]]


def item_path(rule: str, key: str) -> str:
    """File rules key by path; function names never hold `::`, so the path ends at the last one."""
    if rule in TREE_RULES:
        return key.rsplit("::", 1)[0]
    return key


def empty() -> dict:
    return {"version": 1, **{rule: {} for rule in BASELINED}}


def load(path: Path) -> dict:
    if not path.is_file():
        return empty()
    data = json.loads(path.read_text(encoding="utf-8"))
    merged = empty()
    merged.update({rule: dict(data.get(rule, {})) for rule in BASELINED})
    return merged


def dump(data: dict) -> str:
    return json.dumps(data, indent=2, sort_keys=True) + "\n"


def write(path: Path, data: dict) -> None:
    path.write_text(dump(data), encoding="utf-8")


def entry_line(text: str, key: str) -> int:
    needle = json.dumps(key) + ":"
    for number, line in enumerate(text.splitlines(), start=1):
        if needle in line:
            return number
    return 1


@dataclass
class Judge:
    """Compares measured items with their limits and the baseline."""

    config: Config
    data: dict
    text: str = ""

    def item(self, item: Item) -> list[Problem]:
        level = self.config.level(item.rule)
        allowed = limit(self.config, item.rule)
        recorded = self.data[item.rule].get(item.key)
        if item.value <= allowed:
            if recorded is None:
                return []
            message = f"{item.label} is within the limit {allowed}: remove its baseline entry with `godot-guard baseline`"
            return [Problem(item.path, item.line, message, item.rule, level)]
        if recorded is None or item.value > recorded:
            return self.over(item, allowed, recorded, level)
        if item.value < recorded:
            message = f"{item.label}, below its baseline {recorded}: lower it with `godot-guard baseline`"
            return [Problem(item.path, item.line, message, item.rule, level)]
        return []

    def over(self, item: Item, allowed: int, recorded: int | None, level: str) -> list[Problem]:
        if recorded is None:
            suffix = f" (max {allowed})"
        else:
            suffix = f" (baseline {recorded}, max {allowed})"
        if not item.locations:
            return [Problem(item.path, item.line, item.label + suffix, item.rule, level)]
        note = ""
        if recorded is not None:
            note = f"; the file has {item.label}, its baseline allows {recorded}"
        return [Problem(item.path, line, text + note, item.rule, level) for line, text in item.locations]

    def missing(self, results: list[FileResult], subset: set[str] | None) -> list[Problem]:
        """Baseline entries whose item no longer exists, within `subset` (None: everywhere)."""
        present, unparsed = measured(results)
        keys = [(rule, key) for rule, key, _ in entries(self.data) if self.config.level(rule) != "off"]
        return [
            self.gone(rule, key) for rule, key in keys
            if (rule, key) not in present and in_scope(rule, key, subset, unparsed)
        ]

    def gone(self, rule: str, key: str) -> Problem:
        message = f"baseline entry {key!r} no longer exists: remove it with `godot-guard baseline`"
        return Problem(BASELINE_NAME, entry_line(self.text, key), message, rule, self.config.level(rule))


def in_scope(rule: str, key: str, subset: set[str] | None, unparsed: set[str]) -> bool:
    """A missing entry counts when its file is in the subset and parsed."""
    path = item_path(rule, key)
    if subset is not None and path not in subset:
        return False
    return not (rule in PARSED_RULES and path in unparsed)


def judge(config: Config) -> Judge:
    path = config.baseline_path
    text = path.read_text(encoding="utf-8") if path.is_file() else ""
    return Judge(config, load(path), text)


def violations(config: Config, results: list[FileResult]) -> dict:
    """Every item over its limit, as baseline data."""
    data = empty()
    for result in results:
        for item in result.items:
            if config.level(item.rule) != "off" and item.value > limit(config, item.rule):
                data[item.rule][item.key] = item.value
    return data


def measured(results: list[FileResult]) -> tuple[dict, set[str]]:
    """Current values by (rule, key), and the paths that failed to parse."""
    current = {(i.rule, i.key): i.value for r in results for i in r.items}
    return current, {r.path for r in results if not r.parsed}


def entries(data: dict) -> list[tuple[str, str, int]]:
    return [(rule, key, recorded) for rule in BASELINED for key, recorded in sorted(data[rule].items())]


def tighten(config: Config, data: dict, results: list[FileResult]) -> tuple[dict, list[str]]:
    """Drop stale entries and lower loose ones; never add or raise."""
    current, unparsed = measured(results)
    tightened, changes = empty(), []
    for rule, key, recorded in entries(data):
        value = current.get((rule, key))
        if value is None and not in_scope(rule, key, None, unparsed):
            value = recorded
        kept, change = settle(rule, key, recorded, value, limit(config, rule))
        if kept is not None:
            tightened[rule][key] = kept
        if change:
            changes.append(change)
    return tightened, changes


def settle(rule: str, key: str, recorded: int, value: int | None, allowed: int) -> tuple[int | None, str]:
    if value is None:
        return None, f"removed {rule} {key}: no longer exists"
    if value <= allowed:
        return None, f"removed {rule} {key}: {value} is within the limit {allowed}"
    if value < recorded:
        return value, f"lowered {rule} {key}: {recorded} -> {value}"
    return recorded, ""
