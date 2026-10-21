"""Count a GDScript script's configuration surface, as pedro-best-practices `godot_interface.py` does.

A script's configuration surface is what a parent decides when it uses the
script: its top-level `@export` variables (group, subgroup, and category
annotations are not variables), its signals, and the parameters of `_init`
and of a `setup` or `configure` method. Resource scripts are data
definitions and are excluded: a script whose `extends` chain reaches
`Resource` through the repository's `class_name` declarations. Inner classes
are not counted.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

NOT_VARIABLES = {"export_group", "export_subgroup", "export_category"}
SETUP_METHODS = ("_init", "setup", "configure")
TOKENS = re.compile(r"\"[^\"]*(?:\"|\Z)|'[^']*(?:'|\Z)|[()\[\]{},]")
DEPTH = {"(": 1, "[": 1, "{": 1, ")": -1, "]": -1, "}": -1}


@dataclass
class Surface:
    path: str
    extends: str = ""
    class_name: str = ""
    signals: list[str] = field(default_factory=list)
    exports: list[str] = field(default_factory=list)
    parameters: dict[str, list[str]] = field(default_factory=dict)

    @property
    def total(self) -> int:
        return len(self.signals) + len(self.exports) + sum(len(p) for p in self.parameters.values())

    def describe(self) -> str:
        parts = []
        if self.exports:
            parts.append(f"@export {len(self.exports)}")
        if self.signals:
            parts.append(f"signals {len(self.signals)}")
        parts += [f"{method}() {len(names)}" for method, names in self.parameters.items() if names]
        return ", ".join(parts)


def split_top_level(text: str) -> list[str]:
    """Split on commas outside brackets and strings."""
    parts, depth, start = [], 0, 0
    for token in TOKENS.finditer(text):
        mark = token.group()
        depth += DEPTH.get(mark, 0)
        if mark == "," and depth == 0:
            parts.append(text[start : token.start()])
            start = token.end()
    parts.append(text[start:])
    return [p.strip() for p in parts if p.strip()]


def parameters_after(source: str, start: int) -> list[str]:
    """Parameter names of the signature whose `(` is at or after `start`."""
    open_at = source.index("(", start)
    depth = 0
    for index in range(open_at, len(source)):
        depth += {"(": 1, ")": -1}.get(source[index], 0)
        if depth == 0:
            return [re.split(r"[:=\s]", p, maxsplit=1)[0] for p in split_top_level(source[open_at + 1 : index])]
    return []


def read_header(surface: Surface, line: str) -> None:
    found = re.match(r"extends\s+([\w.\"/]+)", line)
    if found:
        surface.extends = found.group(1).strip('"')
    found = re.match(r"class_name\s+(\w+)(?:\s+extends\s+([\w.]+))?", line)
    if found:
        surface.class_name = found.group(1)
        surface.extends = found.group(2) or surface.extends
    found = re.match(r"signal\s+(\w+)", line)
    if found:
        surface.signals.append(found.group(1))


def is_variable(line: str) -> bool:
    return bool(re.search(r"(^|\s)var\s+\w+", line)) and not line.startswith(("func", "static func"))


def is_export_annotation(name: str) -> bool:
    """Whether an annotation is an `@export` of a variable: group, subgroup, and category are not."""
    return name.startswith("export") and name not in NOT_VARIABLES


def is_exported(annotations: list[str]) -> bool:
    return any(is_export_annotation(a) for a in annotations)


def read_exports(surface: Surface, line: str, pending: list[str]) -> list[str]:
    """Record an exported variable on `line`; return the annotations still waiting for a var."""
    annotations = re.findall(r"@(\w+)", line.split("var ", 1)[0]) if line.startswith("@") else []
    if is_variable(line):
        if is_exported(annotations + pending):
            surface.exports.append(re.search(r"var\s+(\w+)", line).group(1))
        return []
    if annotations and "var " not in line:
        return pending + [a for a in annotations if a not in NOT_VARIABLES]
    return []


def parse(path: str, source: str) -> Surface:
    surface = Surface(path)
    pending: list[str] = []
    for match in re.finditer(r"^(?P<line>\S.*)$", source, re.M):
        line = match.group("line")
        if line.startswith("#"):
            continue
        read_header(surface, line)
        pending = read_exports(surface, line, pending)
        found = re.match(r"(?:static\s+)?func\s+(\w+)\s*\(", line)
        if found and found.group(1) in SETUP_METHODS:
            surface.parameters[found.group(1)] = parameters_after(source, match.start())
    return surface


def is_resource(surface: Surface, by_class: dict[str, Surface], by_path: dict[str, Surface]) -> bool:
    seen, current = set(), surface.extends
    while current and current not in seen:
        if current == "Resource":
            return True
        seen.add(current)
        parent = by_class.get(current) or by_path.get(current.replace("res://", ""))
        current = parent.extends if parent else ""
    return False
