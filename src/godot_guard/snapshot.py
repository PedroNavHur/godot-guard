"""The staged scripts as they will be committed, exported from the git index for the pre-commit gate."""

from __future__ import annotations

import dataclasses
import shutil
import subprocess
import tempfile
from contextlib import contextmanager
from pathlib import Path

from .config import BASELINE_NAME, Config

# Each tool takes the first of its names found from the working directory upward.
TOOL_CONFIGS = (("gdlintrc", ".gdlintrc"), ("gdformatrc", ".gdformatrc"))


def git(config: Config, *args: str, stdin: str | None = None) -> str:
    return subprocess.run(
        ["git", *args], cwd=config.root, input=stdin, capture_output=True, text=True, check=True
    ).stdout


def staged(config: Config) -> list[str]:
    """Scanned scripts added, copied, modified, or renamed in the index, relative to the root."""
    names = git(config, "diff", "--cached", "--name-only", "--diff-filter=ACMR", "--relative", "-z").split("\0")
    return sorted(name for name in names if config.is_scanned(name))


def nearest(start: Path, names: tuple[str, ...]) -> Path | None:
    for directory in (start, *start.parents):
        for name in names:
            if (directory / name).is_file():
                return directory / name
    return None


def copy_tool_configs(source: Path, target: Path) -> None:
    """Copy the gdlint and gdformat configs the tools would find from `source` into `target`."""
    for names in TOOL_CONFIGS:
        found = nearest(source, names)
        if found:
            shutil.copy(found, target / found.name)


@contextmanager
def snapshot(config: Config):
    """A config rooted in a temporary copy of the index's scripts and baseline, plus the tool configs.

    The baseline falls back to the working tree's when the index has none.
    """
    top = Path(git(config, "rev-parse", "--show-toplevel").strip()).resolve()
    with tempfile.TemporaryDirectory() as temp:
        base = Path(temp).resolve()
        root = base / config.root.relative_to(top)
        listed = git(config, "ls-files", "-z", "--", "*.gd", BASELINE_NAME)
        git(config, "checkout-index", "-z", "--stdin", f"--prefix={base}/", stdin=listed)
        root.mkdir(parents=True, exist_ok=True)
        copy_tool_configs(config.root, root)
        if BASELINE_NAME not in listed.split("\0") and config.baseline_path.is_file():
            shutil.copy(config.baseline_path, root / BASELINE_NAME)
        yield dataclasses.replace(config, root=root)
