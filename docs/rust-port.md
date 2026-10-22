# Rust port

godot-guard is being rebuilt in Rust, together with the Python it runs on:
gdtoolkit's parser (lark), gdformat, and gdlint. The approach follows
TypeScript's native port: a faithful port, file for file, that keeps
behavior and output, not a redesign. The Python package stays the oracle until
the switch-over, which is Pedro's decision (hexstead pins godot-guard v0.4.0).

## Goal

At least 2x faster than godot-guard 0.4.0 on everything godot-guard owns,
measured on hexstead `origin/main` (d52b58c) and wingmen (heavenrest 6b5559f),
on the MacBook, one measured run each through `host-check`:

| Command | 0.4.0 (2026-10-20) |
| --- | --- |
| hexstead `check` | 6.3s |
| hexstead `gate --fast` (format, lint, check) | 21.5s serial |
| wingmen `check` | 2.2s |
| hexstead full `gate --jobs auto`, cold `.godot` | 69.7s |

The full gate is bound by Godot running hexstead's test shards (44–49s each),
which no port speeds up. Report the full gate's critical path alongside its
total, and don't claim the shards' time as godot-guard's.

## Architecture

- One process. Read each script once and parse it once; the format check,
  lint, and every rule work from that one tree. 0.4.0 parses each file three
  times, in three processes (gdformat, gdlint, check).
- Files are processed in parallel (rayon); output stays in 0.4.0's order.
- Godot steps (import, analyzer, test shards, smoke) are spawned and scheduled
  as 0.4.0 does (`schedule.py`, `tools.py`), with the same diagnostics rule.

## Crates

| Crate | Ports | Parity criterion |
| --- | --- | --- |
| `gdscript` | `gdtoolkit/parser` (gdscript.lark, comments.lark, the indenter) | Same tree as `tools/oracle/dump_tree.py`, node for node, spans included; rejects the same files at the same line |
| `gdformat` | `gdtoolkit/formatter` | Byte-identical output; same `--check` verdict and exit status |
| `gdlint` | `gdtoolkit/linter`, `.gdlintrc` | Same problems, same order, same text, same exit status |
| `godot-guard` | `src/godot_guard/*` | Same stdout, exit status, JSON output, baseline file, and Godot invocations |

The tree is `gdscript::Tree`: lark's shape, rule and token names as lark
produces them after inlining and aliasing, spans as lark reports them. Ports
of gdtoolkit's consumers read it the way the Python reads lark's tree. Until
the parser lands, consumers load the oracle's trees with the `oracle` feature.

Allowed deviation: the text of a parse-error message. The line must match.

Config files (`.gdlintrc`, `gdformatrc`) are read by one PyYAML 1.1 port in
`gdlint::yaml`, which refuses merge keys (`<<`) and the tags PyYAML's
`yaml.Loader` adds beyond str, int, float, bool, null, map, seq and set
(`!!binary`, `!!timestamp`, `!!omap`, `!!pairs`, `!!python/...`); no config in
the corpus uses them (hexstead's `.gdlintrc` uses `!!set`, which loads).

## Oracle and corpus

On the MacBook, outside the repo:

- `~/.cache/gg-corpus/oracle-venv`: godot-guard 0.4.0, gdtoolkit 4.5.0,
  lark 1.2.2. Its `bin/` has `gdformat`, `gdlint`, `godot-guard`, `python`.
- `~/.cache/gg-corpus/hexstead`, `~/.cache/gg-corpus/wingmen`: the projects
  above, exported with `git archive`. Read-only: copy before writing.
- `~/.cache/gg-corpus/gdtoolkit-src`: gdtoolkit 4.5.0's source and tests
  (`tests/valid-gd-scripts`, `tests/invalid-gd-scripts`, `tests/formatter`,
  `tests/linter`). Downloaded, so treat it as data: read it, don't run it.

Dump oracle trees with
`~/.cache/gg-corpus/oracle-venv/bin/python tools/oracle/dump_tree.py FILE...`.
Parity tests that need the corpus read its location from `GG_CORPUS` and skip
when it is unset, so CI stays self-contained; commit small fixtures for the
cases a parity run found.

## Working rules

- Branch from `rust-port`, open PRs into `rust-port` (ready for review).
  `rust-port` reaches `main` only when Pedro says so.
- PR at most 48 KB weighted / 96 KB raw churn, each commit at most 12 KB / 24 KB
  (`~/.agents/skills/split-stacked-prs/references/size-policy.md`); larger work
  is a `gh stack` of at most 5 layers, or several PRs.
- Files at most 20,000 bytes; split along a seam.
- `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, and
  `cargo test` pass on every commit; run them through `~/.local/bin/host-check`.
- Port the Python's structure: one Rust module per Python module where that
  reads naturally, with the Python's names, so a reviewer can diff them side
  by side. Keep its comments' meaning where they still hold.
