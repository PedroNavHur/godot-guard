"""Command line: godot-guard check | baseline | format | run | gate | init."""

from __future__ import annotations

import argparse
import subprocess
import sys
from dataclasses import replace
from pathlib import Path

from . import __version__, config as config_module
from .check import check, select, update_baseline
from .config import AUTO
from .gate import gate
from .scaffold import init
from .tools import ToolError, gdtoolkit, run_godot


def jobs_argument(value: str) -> int | str:
    """`--jobs`: a whole number of at least 1, or `auto`."""
    if value == AUTO:
        return AUTO
    if value.isascii() and value.isdigit() and int(value) >= 1:
        return int(value)
    raise argparse.ArgumentTypeError(f"must be a whole number of at least 1, or {AUTO}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="godot-guard", description="Guardrails for Godot 4 projects.")
    parser.add_argument("--version", action="version", version=f"godot-guard {__version__}")
    commands = parser.add_subparsers(dest="command", required=True)
    check_parser = commands.add_parser("check", help="run the rules against the baseline")
    check_parser.add_argument("--json", action="store_true", help="print one JSON object instead of text")
    check_parser.add_argument("files", nargs="*")
    baseline_parser = commands.add_parser("baseline", help="tighten the baseline, or --init it")
    baseline_parser.add_argument("--init", action="store_true", help="record every current violation")
    baseline_parser.add_argument("--force", action="store_true", help="let --init overwrite the baseline")
    format_parser = commands.add_parser("format", help="gdformat the files or the scanned paths")
    format_parser.add_argument("files", nargs="*")
    run_parser = commands.add_parser("run", help="run Godot; fail on errors and warnings")
    run_parser.add_argument("args", nargs=argparse.REMAINDER)
    gate_parser = commands.add_parser("gate", help="the quality gate")
    gate_parser.add_argument("--fast", action="store_true", help="format, lint, and check only")
    gate_parser.add_argument("--staged", action="store_true", help="select staged .gd files")
    gate_parser.add_argument("--jobs", type=jobs_argument, metavar="N|auto",
                             help="run steps in parallel on N workers, or auto; overrides gate.jobs")
    gate_parser.add_argument("files", nargs="*")
    commands.add_parser("init", help="scaffold config, hooks, and CI")
    return parser


def format_files(config, files: list[str]) -> int:
    paths = config.scanned_files() if not files else select(config, files)
    if not paths:
        print("godot-guard format: no GDScript files selected")
        return 0
    return subprocess.run([gdtoolkit("gdformat"), *paths], cwd=config.root).returncode


def run_gate(config, args: argparse.Namespace) -> int:
    if args.staged and args.files:
        raise SystemExit("godot-guard gate: pass --staged or files, not both")
    if args.jobs is not None:
        config = replace(config, gate={**config.gate, "jobs": args.jobs})
    return gate(config, args.fast, args.staged, args.files)


CONFIG_COMMANDS = {
    "check": lambda config, args: check(config, args.files or None, as_json=args.json),
    "baseline": lambda config, args: update_baseline(config, args.init, args.force),
    "format": lambda config, args: format_files(config, args.files),
    "gate": run_gate,
}


def run_command(args: argparse.Namespace) -> int:
    if args.command == "run":
        godot_args = args.args[1:] if args.args[:1] == ["--"] else args.args
        return run_godot(godot_args)
    if args.command == "init":
        root, _ = config_module.find_root(Path.cwd())
        return init(root)
    return CONFIG_COMMANDS[args.command](config_module.load(), args)


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return run_command(args)
    except (config_module.ConfigError, ToolError) as error:
        print(f"godot-guard: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
