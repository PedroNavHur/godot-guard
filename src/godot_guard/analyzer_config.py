"""`analyzer-config`: project.godot must set the analyzer warnings in `godot_warnings.REQUIRED` to error.

The analyzer step fails only on warnings at error: Godot prints nothing for a lower level when it
loads a script headless, so a warning left at warn is never checked.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from .config import Config, matches
from .godot_warnings import ERROR, REQUIRED
from .rules import Problem

PROJECT = "project.godot"
RULE = "analyzer-config"
SECTION = re.compile(r"\[([^\]]+)\]\s*$")
WARNING = re.compile(r"gdscript/warnings/(\w+)\s*=\s*(.*?)\s*$")
DIRECTORY_RULES = re.compile(r"gdscript/warnings/directory_rules\s*=\s*\{([^}]*)\}")
DIRECTORY_ENTRY = re.compile(r'"res://([^"]*)"\s*:\s*(\d+)')


@dataclass(frozen=True)
class Setting:
    line: int
    value: str


def debug_lines(text: str) -> list[tuple[int, str]]:
    """The `[debug]` section's numbered lines, its header first."""
    inside, found = False, []
    for number, line in enumerate(text.splitlines(), start=1):
        header = SECTION.match(line)
        if header:
            inside = header.group(1) == "debug"
        if inside:
            found.append((number, line))
    return found


def read_settings(lines: list[tuple[int, str]]) -> dict[str, Setting]:
    """`gdscript/warnings/*` settings by name."""
    matched = ((number, WARNING.match(line)) for number, line in lines)
    return {found.group(1): Setting(number, found.group(2)) for number, found in matched if found}


def lowered(name: str, setting: Setting) -> tuple[int, str]:
    message = f"gdscript/warnings/{name} is {setting.value}; set it to {ERROR}: the analyzer step ignores lower levels"
    return setting.line, message


def absent(names: list[str], line: int) -> tuple[int, str]:
    message = (f"{len(names)} analyzer warnings default below error: add gdscript/warnings/NAME={ERROR} "
               f"under [debug] for {', '.join(names)}")
    return line, message


def disabled(settings: dict[str, Setting]) -> list[tuple[int, str]]:
    enable = settings.get("enable")
    if enable is None or enable.value == "true":
        return []
    return [(enable.line, "gdscript/warnings/enable turns every analyzer warning off; remove the setting")]


def directory_rules_off(lines: list[tuple[int, str]], config: Config) -> list[tuple[int, str]]:
    """(line, message) for each `directory_rules` entry that turns warnings off under a path the scan covers."""
    body = "\n".join(line for _, line in lines)
    found = []
    for setting in DIRECTORY_RULES.finditer(body):
        line = lines[body.count("\n", 0, setting.start())][0]
        for path, level in DIRECTORY_ENTRY.findall(setting.group(1)):
            if level == "0" and not matches(path + "/", config.exclude):
                message = (f"gdscript/warnings/directory_rules turns analyzer warnings off under res://{path}; "
                           "remove that entry")
                found.append((line, message))
    return found


def findings(settings: dict[str, Setting], relaxed: list[str], header: int) -> list[tuple[int, str]]:
    """(line, message) for warnings off, each required one set below error, and those left at a lower default."""
    found = disabled(settings)
    checked = [name for name in REQUIRED if name not in relaxed]
    found += [lowered(name, settings[name]) for name in checked if name in settings and settings[name].value != str(ERROR)]
    missing = [name for name in checked if name not in settings and REQUIRED[name] != ERROR]
    if missing:
        found.append(absent(missing, header))
    return found


def problems(config: Config) -> list[Problem]:
    """The project's analyzer settings; none without a project.godot or with the rule off."""
    path = config.root / PROJECT
    level = config.level(RULE)
    if level == "off" or not path.is_file():
        return []
    lines = debug_lines(path.read_text(encoding="utf-8", errors="replace"))
    header = lines[0][0] if lines else 1
    found = findings(read_settings(lines), config.relaxed_warnings, header)
    found += directory_rules_off(lines, config)
    return [Problem(PROJECT, line, message, RULE, level) for line, message in found]
