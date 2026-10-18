"""Load and validate `.godot-guard.yml`, and select the scripts it scans."""

from __future__ import annotations

import copy
import fnmatch
import os
import re
from dataclasses import dataclass, field
from pathlib import Path

import yaml

CONFIG_NAME = ".godot-guard.yml"
BASELINE_NAME = ".godot-guard-baseline.json"
LEVELS = ("error", "warn", "off")
BASELINED = ("cognitive-complexity", "cyclomatic-complexity", "file-lines", "long-comment", "interface-cap")
DEFAULTS = {
    "paths": ["."],
    "exclude": ["addons/**", ".godot/**"],
    "limits": {"cognitive": 8, "cyclomatic": 16, "file-lines": 300, "comment-lines": 2, "inputs": 8},
    "rules": {
        "cognitive-complexity": "error",
        "cyclomatic-complexity": "error",
        "file-lines": "error",
        "long-comment": "error",
        "interface-cap": "error",
        "nested-ternary": "warn",
    },
    "boundaries": [],
    "gate": {
        "format": True,
        "lint": True,
        "steps": [],
        "import": True,
        "analyzer": True,
        "tests": [],
        "smoke": ["--quit-after", "3"],
        "jobs": 1,
    },
}
BOUNDARY_KEYS = {"name", "paths", "forbid", "message"}
TEST_KEYS = {"script", "shards"}
AUTO = "auto"
MERGE_TAG = "tag:yaml.org,2002:merge"


class ConfigError(Exception):
    pass


@dataclass(frozen=True)
class TestScript:
    """A `gate.tests` entry: a res:// script, run as `shards` Godot processes numbered 1 to `shards`."""

    script: str
    shards: int = 1


class StrictLoader(yaml.SafeLoader):
    """A safe loader without anchors, aliases, or merge keys, that rejects a key repeated in one mapping."""

    def compose_node(self, parent, index):
        event = self.peek_event()
        if event.anchor is not None:
            raise yaml.composer.ComposerError(None, None, "anchors and aliases are not supported", event.start_mark)
        return super().compose_node(parent, index)

    def construct_mapping(self, node, deep=False):
        seen = []
        for key_node, _ in node.value:
            if key_node.tag == MERGE_TAG:
                raise yaml.constructor.ConstructorError(None, None, "merge keys are not supported", key_node.start_mark)
            key = self.construct_object(key_node, deep=deep)
            if key in seen:
                raise yaml.constructor.ConstructorError(None, None, f"duplicate key {key!r}", key_node.start_mark)
            seen.append(key)
        return super().construct_mapping(node, deep)


@dataclass
class Config:
    root: Path
    paths: list[str] = field(default_factory=lambda: list(DEFAULTS["paths"]))
    exclude: list[str] = field(default_factory=lambda: list(DEFAULTS["exclude"]))
    limits: dict = field(default_factory=lambda: dict(DEFAULTS["limits"]))
    rules: dict = field(default_factory=lambda: dict(DEFAULTS["rules"]))
    boundaries: list = field(default_factory=list)
    gate: dict = field(default_factory=lambda: copy.deepcopy(DEFAULTS["gate"]))

    @property
    def baseline_path(self) -> Path:
        return self.root / BASELINE_NAME

    def level(self, rule: str) -> str:
        return self.rules.get(rule, "error")

    def is_scanned(self, relative: str) -> bool:
        if not relative.endswith(".gd") or hidden(relative) or matches(relative, self.exclude):
            return False
        return any(p == "." or relative == p or relative.startswith(p.rstrip("/") + "/") for p in self.paths)

    def scanned_files(self) -> list[str]:
        found = set()
        for entry in self.paths:
            found.update(r for r in walk_scripts(self.root, self.root / entry) if self.is_scanned(r))
        return sorted(found)


def hidden(relative: str) -> bool:
    """Whether a directory on the root-relative path is hidden; a trailing `/` marks the path itself as one."""
    return any(part.startswith(".") for part in relative.split("/")[:-1])


def matches(relative: str, patterns: list[str]) -> bool:
    """fnmatch globs (`*` crosses `/`), and a bare directory matches everything below it."""
    return any(fnmatch.fnmatchcase(relative, p) or relative.startswith(p.rstrip("/") + "/") for p in patterns)


def walk_scripts(root: Path, start: Path) -> list[str]:
    """Repo-relative `.gd` paths under `start`, skipping hidden directories."""
    if start.is_file():
        return [start.relative_to(root).as_posix()]
    found = []
    for directory, subdirectories, files in os.walk(start):
        subdirectories[:] = sorted(d for d in subdirectories if not d.startswith("."))
        base = Path(directory).relative_to(root)
        found += [(base / name).as_posix() for name in files if name.endswith(".gd")]
    return found


def find_root(start: Path) -> tuple[Path, bool]:
    """The nearest directory holding the config, else one holding project.godot."""
    start = start.resolve()
    candidates = [start, *start.parents]
    for directory in candidates:
        if (directory / CONFIG_NAME).is_file():
            return directory, True
    for directory in candidates:
        if (directory / "project.godot").is_file():
            return directory, False
    raise ConfigError(f"no {CONFIG_NAME} or project.godot in {start} or its parents")


def load(start: Path | None = None) -> Config:
    root, has_config = find_root(start or Path.cwd())
    config = Config(root)
    if not has_config:
        return config
    try:
        data = yaml.load((root / CONFIG_NAME).read_text(encoding="utf-8"), Loader=StrictLoader)
    except yaml.YAMLError as error:
        raise ConfigError(f"{CONFIG_NAME}: {yaml_problem(error)}") from error
    apply(config, {} if data is None else data)
    return config


def yaml_problem(error: yaml.YAMLError) -> str:
    """The error on one line, with the line it points at."""
    problem = getattr(error, "problem", None) or str(error).splitlines()[0]
    mark = getattr(error, "problem_mark", None)
    if mark is None:
        return problem
    return f"line {mark.line + 1}: {problem}"


def apply(config: Config, data) -> None:
    expect(isinstance(data, dict), "the file must be a mapping")
    unknown(data, DEFAULTS, "")
    config.paths = [normalize(p) for p in string_list(given(data, "paths", config.paths), "paths")]
    config.exclude = [normalize(p) for p in string_list(given(data, "exclude", config.exclude), "exclude")]
    config.limits.update(section(data, "limits"))
    config.rules.update({rule: level_of(value) for rule, value in section(data, "rules").items()})
    config.gate.update(section(data, "gate"))
    entries = given(data, "boundaries", [])
    expect(isinstance(entries, list), "boundaries must be a list")
    config.boundaries = [boundary(entry) for entry in entries]
    validate(config)


def normalize(path: str) -> str:
    path = path.strip().rstrip("/")
    while path.startswith("./"):
        path = path[2:]
    return path or "."


def level_of(value):
    """YAML 1.1 reads a bare `off` as false."""
    if value is False:
        return "off"
    return value


def given(data: dict, key: str, default):
    """An omitted key or an explicit null takes the default; any other value is checked by the caller."""
    value = data.get(key)
    return default if value is None else value


def section(data: dict, name: str) -> dict:
    values = given(data, name, {})
    expect(isinstance(values, dict), f"{name} must be a mapping")
    unknown(values, DEFAULTS[name], f"{name}.")
    return {key: value for key, value in values.items() if value is not None}


def unknown(values: dict, known, prefix: str) -> None:
    for key in values:
        expect(key in known, f"unknown key '{prefix}{key}'")


def string_list(value, name: str) -> list[str]:
    expect(isinstance(value, list) and all(isinstance(v, str) for v in value), f"{name} must be a list of strings")
    return value


def whole_count(value) -> bool:
    """A whole number of at least 1; a bool is not a number here."""
    return isinstance(value, int) and not isinstance(value, bool) and value >= 1


def script_entry(entry) -> TestScript:
    """A `gate.tests` entry: a script path, or a mapping with `script` and an optional `shards` count."""
    if isinstance(entry, str):
        return TestScript(entry)
    expect(isinstance(entry, dict), "each gate.tests entry must be a string or a mapping")
    unknown(entry, TEST_KEYS, "gate.tests.")
    script = given(entry, "script", None)
    expect(isinstance(script, str), "gate.tests script must be a string")
    shards = given(entry, "shards", 1)
    expect(whole_count(shards), "gate.tests shards must be a whole number of at least 1")
    return TestScript(script, shards)


def expect(condition: bool, message: str) -> None:
    if not condition:
        raise ConfigError(f"{CONFIG_NAME}: {message}")


def boundary(entry) -> dict:
    expect(isinstance(entry, dict), "each boundary must be a mapping")
    for key in entry:
        expect(key in BOUNDARY_KEYS, f"unknown key 'boundaries.{key}'")
    name = given(entry, "name", "boundary")
    for pattern in string_list(given(entry, "forbid", []), f"boundary {name} forbid"):
        try:
            re.compile(pattern)
        except re.error as error:
            raise ConfigError(f"{CONFIG_NAME}: boundary {name}: bad pattern {pattern!r}: {error}") from error
    return {
        "name": name,
        "paths": string_list(given(entry, "paths", []), f"boundary {name} paths"),
        "forbid": string_list(given(entry, "forbid", []), f"boundary {name} forbid"),
        "message": str(given(entry, "message", "")),
    }


def validate(config: Config) -> None:
    for key, value in config.limits.items():
        expect(isinstance(value, int) and not isinstance(value, bool) and value >= 0, f"limits.{key} must be a whole number")
    for rule, level in config.rules.items():
        expect(level in LEVELS, f"rules.{rule} must be one of {', '.join(LEVELS)}")
    expect(config.rules["nested-ternary"] != "error", "rules.nested-ternary is advisory: use warn or off")
    gate = config.gate
    for key in ("format", "lint", "import", "analyzer"):
        expect(isinstance(gate[key], bool), f"gate.{key} must be true or false")
    jobs = gate["jobs"]
    expect(jobs == AUTO or whole_count(jobs), f"gate.jobs must be a whole number of at least 1, or {AUTO}")
    string_list(gate["steps"], "gate.steps")
    expect(isinstance(gate["tests"], list), "gate.tests must be a list")
    gate["tests"] = [script_entry(entry) for entry in gate["tests"]]
    smoke = gate["smoke"]
    expect(smoke is False or isinstance(smoke, list), "gate.smoke must be a list of arguments or false")
    if smoke:
        gate["smoke"] = [str(arg) for arg in smoke]
