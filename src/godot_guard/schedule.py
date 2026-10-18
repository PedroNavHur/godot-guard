"""Gate steps: what each one is, and how they run in order."""

from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Callable

from .tools import Echo, ToolError, write_stdout

# Phases run in this order; steps within one phase may run concurrently.
PREPARE = "prepare"  # format, lint, check, and the gate.steps commands
IMPORT = "import"  # Godot's import, alone
TEST = "test"  # the analyzer, every test process, and smoke


@dataclass(frozen=True)
class Step:
    name: str
    phase: str
    # action(echo, env): echo None writes to the terminal; env is None or a Godot process's environment.
    action: Callable[[Echo | None, dict | None], int]
    godot: bool = False


def run_steps(steps: list[Step]) -> int:
    started = time.monotonic()
    for step in steps:
        print(f"== {step.name}", flush=True)
        began = time.monotonic()
        status = run_step(step, None, None)
        elapsed = time.monotonic() - began
        if status:
            print(f"== {step.name} failed after {elapsed:.1f}s", flush=True)
            return 1
        print(f"== {step.name} passed in {elapsed:.1f}s", flush=True)
    print(f"gate passed in {time.monotonic() - started:.1f}s")
    return 0


def run_step(step: Step, echo: Echo | None, env: dict | None) -> int:
    """The step's status. A missing tool fails it, with the reason written to its output."""
    try:
        return step.action(echo, env)
    except ToolError as error:
        (echo or write_stdout)(f"godot-guard: {error}\n")
        return 127
