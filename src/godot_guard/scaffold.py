"""`godot-guard init`: scaffold the config, hooks, CI workflow, and requirement line."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path

from . import __version__
from .config import CONFIG_NAME

TEMPLATES = Path(__file__).parent / "templates"
FALLBACK_GODOT = "4.7.1"
REQUIREMENT = f"godot-guard @ git+https://github.com/PedroNavHur/godot-guard@v{__version__}"


def godot_version(root: Path) -> str:
    """The project's `config/features` version; a bare minor of the fallback keeps its patch."""
    project = root / "project.godot"
    text = project.read_text(encoding="utf-8") if project.is_file() else ""
    found = re.search(r'config/features=PackedStringArray\("(\d+\.\d+(?:\.\d+)?)"', text)
    if not found or FALLBACK_GODOT.startswith(found.group(1) + "."):
        return FALLBACK_GODOT
    return found.group(1)


def template(name: str, **values: str) -> str:
    text = (TEMPLATES / name).read_text(encoding="utf-8")
    for key, value in values.items():
        text = text.replace("{" + key + "}", value)
    return text


def planned(root: Path) -> list[tuple[str, str]]:
    tests = "[res://tests/run_tests.gd]" if (root / "tests" / "run_tests.gd").is_file() else "[]"
    return [
        (CONFIG_NAME, template("godot-guard.yml", tests=tests)),
        (".githooks/pre-commit", template("pre-commit")),
        (".githooks/pre-push", template("pre-push")),
        (".github/workflows/quality.yml", template("quality.yml", godot_version=godot_version(root))),
    ]


def write_new(root: Path, relative: str, text: str) -> None:
    path = root / relative
    if path.exists():
        print(f"skipped {relative}: already exists")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    if relative.startswith(".githooks/"):
        path.chmod(0o755)
    print(f"wrote {relative}")


def add_requirement(root: Path) -> None:
    path = root / "tools" / "requirements-lint.txt"
    lines = path.read_text(encoding="utf-8").splitlines() if path.is_file() else []
    if any(line.strip().startswith("godot-guard") for line in lines):
        print("skipped tools/requirements-lint.txt: already requires godot-guard")
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join([*lines, REQUIREMENT]) + "\n", encoding="utf-8")
    print(f"added `{REQUIREMENT}` to tools/requirements-lint.txt")


def init(root: Path) -> int:
    for relative, text in planned(root):
        write_new(root, relative, text)
    add_requirement(root)
    result = subprocess.run(["git", "config", "core.hooksPath", ".githooks"], cwd=root, capture_output=True, text=True)
    if result.returncode == 0:
        print("ran git config core.hooksPath .githooks")
    else:
        print("not a git repository: run `git config core.hooksPath .githooks` once it is")
    return 0
