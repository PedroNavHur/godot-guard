"""`gate.jobs: auto`: how many gate steps this process can run at once."""

from __future__ import annotations

import math
import os
from pathlib import Path

CGROUP_ROOT = Path("/sys/fs/cgroup")
PROC_CGROUP = Path("/proc/self/cgroup")


def usable_cpus() -> int:
    if hasattr(os, "sched_getaffinity"):
        return len(os.sched_getaffinity(0))
    return os.cpu_count() or 1


def own_cgroup(proc_cgroup: Path) -> str | None:
    """This process's cgroup v2 path from the `0::/path` line; None where there is no v2 hierarchy."""
    try:
        lines = proc_cgroup.read_text().splitlines()
    except OSError:
        return None
    return next((line[3:] for line in lines if line.startswith("0::")), None)


def cpu_max(file: Path) -> int | None:
    """Whole CPUs a `cpu.max` file allows, rounded up; None when it is absent or says `max`."""
    try:
        quota, period = file.read_text().split()
    except FileNotFoundError:
        return None
    if quota == "max":
        return None
    return math.ceil(int(quota) / int(period))


def cpu_limits(root: Path, path: str) -> list[int]:
    """The limits of the cgroup at `path` and each ancestor up to `root`."""
    limits = []
    directory = root / path.lstrip("/")
    while True:
        limit = cpu_max(directory / "cpu.max")
        if limit is not None:
            limits.append(limit)
        if directory == root or directory.parent == directory:
            return limits
        directory = directory.parent


def auto_jobs(root: Path = CGROUP_ROOT, proc_cgroup: Path = PROC_CGROUP) -> int:
    """The usable CPUs, or the tightest cgroup quota above this process if lower; never below 1."""
    path = own_cgroup(proc_cgroup)
    limits = cpu_limits(root, path) if path is not None else []
    return max(1, min([usable_cpus(), *limits]))
