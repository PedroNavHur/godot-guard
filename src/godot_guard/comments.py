"""Comment runs and code-line counts, ported from pedro-best-practices `long_comments.py` (GDScript only).

A comment is an unbroken run of `#` lines that each own their line. Prose
lines exclude blank lines, separator rules, references (a lone URL or path, a
ticket such as ENG-123), `Run:` lines, and tooling directives. `##` doc
comments, trailing comments, license text, and the file header block above
the first line of code are exempt.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

DIRECTIVE = re.compile(
    r"^(!|-\*-|(es|ox|ts|style)lint|biome-|prettier-|istanbul |c8 |svelte-ignore|@ts-|global |"
    r"noqa|type:|pyright:|mypy:|pylint:|fmt:|isort:|ruff:|pragma|phpcs|@?phpstan|@?psalm|@codeCoverage|"
    r"gdlint|warning_ignore|#?(end)?region\b|run:)", re.IGNORECASE)
REFERENCE = re.compile(r"^(see:?\s+)?(https?://\S+|[\w.-]*/[\w./#-]+|[A-Z][A-Z0-9]+-\d+)[.)]?$", re.IGNORECASE)
SEPARATOR = re.compile(r"^[-=*#/_~+]{3,}$")
LICENSE = re.compile(r"SPDX-License-Identifier|Copyright \(c\)|Copyright \d{4}|Licensed under", re.IGNORECASE)
PREAMBLE = re.compile(r"^(extends\b|class_name\b|@tool\b|@icon\b|##|$)")


@dataclass
class Comment:
    line: int
    end_line: int
    body: list[str]

    @property
    def prose(self) -> int:
        return sum(1 for text in self.body if is_prose(text))


def is_prose(text: str) -> bool:
    text = text.strip().lstrip("*").strip()
    return bool(text) and not (DIRECTIVE.match(text) or REFERENCE.match(text) or SEPARATOR.match(text))


def comment_text(stripped: str) -> str | None:
    """A line comment's text, or None when the line is not one."""
    if stripped.startswith("#") and not stripped.startswith("##"):
        return stripped[1:]
    return None


def comment_runs(lines: list[str]) -> list[Comment]:
    runs: list[Comment] = []
    run: Comment | None = None
    for number, raw in enumerate(lines, start=1):
        text = comment_text(raw.strip())
        if text is None:
            run = None
            continue
        if run is None:
            run = Comment(number, number, [])
            runs.append(run)
        run.end_line = number
        run.body.append(text)
    return runs


def first_code_line(lines: list[str], comments: list[Comment]) -> int:
    commented = {n for c in comments for n in range(c.line, c.end_line + 1)}
    for number, raw in enumerate(lines, start=1):
        if number not in commented and not PREAMBLE.match(raw.strip()):
            return number
    return len(lines) + 1


def long_comments(source: str, cap: int) -> list[Comment]:
    """Comments over `cap` prose lines, outside the file header block."""
    lines = source.splitlines()
    comments = comment_runs(lines)
    first_code = first_code_line(lines, comments)
    return [
        c for c in comments
        if c.prose > cap and c.end_line >= first_code and not LICENSE.search("\n".join(c.body))
    ]


def code_lines(source: str) -> int:
    """Lines that are neither blank nor comment-only."""
    return sum(1 for raw in source.splitlines() if raw.strip() and not raw.strip().startswith("#"))
