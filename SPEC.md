# godot-guard v0.4 spec

godot-guard gives Godot 4 projects the guardrails Pedro's Next.js repositories
get from oxlint, oxfmt, tsc, vitest, Husky, and CI (reference: govstructure's
`.oxlintrc.json`, `.husky/`, `.github/workflows/verify.yml`). It is a small
Python package, installed from `git+https://github.com/PedroNavHur/godot-guard@vX.Y.Z`
next to `gdtoolkit` in each game's `tools/requirements-lint.txt`. First users:
hexstead (`PedroNavHur/hexstead`) and wingmen (`PedroNavHur/heavenrest`).

## Mapping

| Next.js guardrail | Godot equivalent | Owner |
| --- | --- | --- |
| oxfmt / `format:check` | `gdformat --check` | gdtoolkit |
| oxlint core rules | `gdlint` with `.gdlintrc` | gdtoolkit |
| `tsc` strict, `no-unsafe-*` | GDScript analyzer warnings set to error in `project.godot`, loaded headless | Godot |
| `strict: true` in tsconfig | `analyzer-config` (those warnings stay at error) | godot-guard |
| `complexity` max 16, `cognitive-complexity` max 8 | `cognitive-complexity`, `cyclomatic-complexity` | godot-guard |
| `max-lines` | none: fleet's `file-size` check caps every file at 20,000 bytes, in any language | fleet |
| `no-long-comment` | `long-comment` | godot-guard |
| prop cap (pedro/eight-prop-cap) | `interface-cap` (pedro/godot-interface-cap) | godot-guard |
| `no-nested-ternary` advisory | `nested-ternary` (warning, never fails) | godot-guard |
| import-boundary rules | `boundary` (forbidden patterns per directory) | godot-guard |
| vitest | the repo's own headless test scripts | repo |
| Husky + lint-staged | `.githooks/` via `core.hooksPath` | godot-guard templates |
| verify.yml | `.github/workflows/quality.yml` | godot-guard templates |

## Constraints

- Python 3.9+ (`from __future__ import annotations`, no `match`). Runtime
  dependencies: `gdtoolkit>=4.5,<5` only. It brings `lark` (parser) and
  `PyYAML` (config). Tests use stdlib `unittest`.
- Follow Pedro's own limits in this code: functions at cognitive 8 /
  cyclomatic 16 or below, no nested conditional expressions, comments at most
  two prose lines. Verify with
  `~/.agents/skills/pedro-best-practices/scripts/measure_complexity.py EMPTY --repo .`
  and `long_comments.py EMPTY --repo .`.
- Output lines are `path:line: message (rule)` so editors and agents can jump.
  Paths are repo-relative POSIX.
- Reuse, don't reinvent: port the scorers from
  `~/.agents/skills/pedro-best-practices/scripts/` (`complexity_core.py`,
  `complexity_gdscript.py`, `godot_interface.py`, the GDScript parts of
  `long_comments.py`). The complexity scores must match
  `complexity_gdscript.py` exactly on every function of hexstead and wingmen
  (parity test, below). The interface counts must match `godot_interface.py`.

## Config: `.godot-guard.yml` at the repo root

```yaml
# Every key is optional; these are the defaults.
paths: [.]                # directories scanned for *.gd
exclude: [addons/**, .godot/**]
limits:
  cognitive: 8
  cyclomatic: 16
  comment-lines: 2        # prose lines per comment run
  inputs: 8               # interface cap
rules:                    # set a rule to off to disable it
  cognitive-complexity: error
  cyclomatic-complexity: error
  long-comment: error
  interface-cap: error
  nested-ternary: warn    # Pedro's rule: advisory, never a merge blocker
  analyzer-config: error
  lookup-in-hot-path: error
  load-in-hot-path: error
  private-export: error
  self-assign: error
  per-frame-allocation: warn  # advisory, like nested-ternary
relaxed-warnings: []      # analyzer warnings project.godot may keep below error
boundaries: []            # see "boundary"
gate:
  format: true            # gdformat --check <paths>
  lint: true              # gdlint <paths>
  steps: []               # extra shell commands, run from the repo root, before Godot
  import: true            # godot --headless --editor --quit
  analyzer: true          # load every scanned script headless
  tests: []               # res:// scripts, or {script, shards}: run with --script, in order
  test-commands: []       # shell commands, run from the repo root, after tests (0.7.0)
  smoke: [--quit-after, "3"]   # args for a main-scene run; [] or false skips it
  jobs: 1                 # gate workers: a whole number of at least 1, or auto
```

Unknown keys are errors (catch typos), and so is a key repeated in one
mapping at any depth (reported with its line). Anchors, aliases, and `<<`
merge keys are not supported and are errors too. Every YAML error is one line
naming the line it points at. An omitted key or an explicit null
takes its default; any other value of the wrong type (a top-level `false` or
`[]`, `limits: false`, `paths: {}`) is an error naming the key and the
expected type. Rule levels: `error`, `warn`, `off`.

## Rules

[RULES.md](RULES.md) defines each rule `check` runs.

## Ratchet baseline: `.godot-guard-baseline.json`

```json
{
  "version": 1,
  "cognitive-complexity": {"ui/fx.gd::draw_foliage": 33},
  "cyclomatic-complexity": {},
  "long-comment": {"ui/ui_kit.gd": 3},
  "interface-cap": {"scripts/ui/action_bar.gd": 15}
}
```

Sorted keys, two-space indent, trailing newline, so diffs stay small.

- A baselined item may not exceed its recorded value. Above the limit and not
  baselined is an error.
- A baselined item now at or below its limit, or missing (function renamed or
  removed, file deleted), is a **stale** error: "remove it with
  `godot-guard baseline`". A baselined item that improved but is still over
  is a **loose** error: "lower it with `godot-guard baseline`". The file only
  ever shrinks or tightens. This mirrors ESLint's unused-suppression errors.
- `godot-guard baseline` rewrites the file, removing stale entries and lowering
  loose ones. It never adds or raises an entry, and prints what it changed.
- `godot-guard baseline --init` writes every current violation. It refuses to
  overwrite an existing baseline unless `--force` is given. The adoption PR
  and an intentional limit change are its only uses.
- When `check` runs on a subset of files, stale and loose detection applies
  only to entries whose path is in the subset. A file rule's key is its whole
  path; a complexity key's path ends at its last `::`, so paths may contain
  `::`.

## CLI: `godot-guard`

Run from anywhere inside the repo; the root is the nearest directory holding
`.godot-guard.yml` (fall back to `project.godot`, with default config).

- `godot-guard check [FILE...]`: run the rules on the given `.gd` files (or
  every scanned file), against the baseline. Here and in `format` and `gate`,
  a directory argument stands for the scanned scripts below it. Scripts in
  hidden directories (relative to the root) and `exclude` matches are never
  scanned, so a file or directory argument that is hidden, excluded, or below
  a hidden or excluded directory selects nothing and prints a `skipping` note
  on stderr; `--staged` skips such scripts too. An argument is judged by its
  path relative to the root as written (normalized, symlinks below the root
  not resolved), so a link cannot make a hidden or excluded argument
  eligible. Exit 1 on any error; warnings
  print but exit 0. End with a one-line summary: errors, warnings, files.
- `godot-guard check --json [FILE...]`: the same selection, rules, baseline
  judgment, and exit status as `check`, for tools that tag their own findings
  with godot-guard's verdict. stdout is one JSON object and nothing else (no
  summary line); `skipping` notes stay on stderr, and a config error prints
  there and exits 2 with nothing on stdout.

  ```json
  {
    "version": 1,
    "root": "/absolute/path/to/the/root",
    "limits": {"cognitive": 8, "cyclomatic": 16, "comment-lines": 2, "inputs": 8},
    "levels": {"cognitive-complexity": "error", "cyclomatic-complexity": "error",
               "long-comment": "error", "interface-cap": "error", "nested-ternary": "warn"},
    "files": ["scripts/unit.gd"],
    "problems": [
      {"path": "scripts/unit.gd", "line": 12, "rule": "cognitive-complexity", "level": "error",
       "message": "pick has cognitive complexity 9 (max 8)"}
    ]
  }
  ```

  `limits` and `levels` are the effective settings: the defaults with the
  config file applied. `files` lists the repo-relative scripts actually
  checked, after the hidden and `exclude` filters. `problems` holds every
  problem `check` prints, in the same order, including stale, loose, and
  missing baseline entries; `level` is `error` or `warn`, and `message` is the
  text without the `warning: ` prefix and the rule. A complexity problem sits
  on the function's first line and a long-comment problem on the comment's
  first line; a loose long-comment entry sits on the file's first long comment,
  and a missing entry on its line in the baseline file. Other file-rule
  problems, and a stale long-comment entry, sit on line 1. `boundary` and
  `parse-error` have no level setting and are always `error`.

  `version` is the format's version. It rises only when a key is removed or
  its meaning changes; new keys may appear without a bump, so readers ignore
  keys they do not know and refuse a version they do not.
- `godot-guard baseline [--init [--force]]`: as above.
- `godot-guard format [FILE...]`: `gdformat` in write mode on the files or
  paths. For the pre-commit skill's format step.
- `godot-guard run -- ARGS...`: run Godot (resolved as below) with ARGS,
  stream output, and fail when Godot exits non-zero or prints a line
  containing `SCRIPT ERROR:` or starting with `ERROR:` or `WARNING:`. This is
  hexstead's `tools/check_godot_output.py`.
- `godot-guard gate [--fast] [--staged | FILE...] [--jobs N|auto]`:
  - `--fast` (pre-commit): `gdformat --check`, `gdlint`, and `check` on the
    selected files only. `--staged` selects the scanned `.gd` files added,
    copied, modified, renamed, or retyped in the index (`git diff --cached
    --name-only --diff-filter=ACMRT`), including ones since deleted from the
    working tree, and checks what will be committed. Symlinked scripts are
    not supported in staged mode: each staged one fails the gate (exit 1) with
    `PATH: symlinked scripts are not supported; commit the real file`. Full
    scans and file arguments read a symlinked script through its link.
    The index's content of the selected scripts and the index's baseline (the
    working tree's baseline when the index has none) are exported to a
    temporary copy of the root, with the gdlint and gdformat configs the tools
    would find from the root. gdformat, gdlint, and the rules run there, and
    paths print repo-relative. The Resource exclusion's
    `extends`/`class_name` map follows the index: it reads the working tree's
    copy of every other script in the index below the root (`git ls-files
    --recurse-submodules`, so submodule scripts count; hidden directories
    do not), then the exported content of the selected ones, whose paths and
    `class_name`s win. A script staged for deletion is left out, and so is
    one the map cannot read, such as a dangling symlink.
    Nothing selected means nothing to do, exit 0.
  - full (default, pre-push and CI): the whole configured gate in this order,
    stopping at the first failure: format, lint, check, steps, import,
    analyzer, tests, test-commands, smoke. Godot steps go through `run`. Print
    a header per step and the elapsed seconds per step.
  - Jobs: `gate.jobs` sets the worker count, and `--jobs` overrides it for
    the run. A whole number is used as is; `auto` is the usable CPUs (the
    affinity mask's size where `os.sched_getaffinity` exists, else
    `os.cpu_count()`), lowered to the tightest `cpu.max` quota on the
    process's cgroup v2 path and its ancestors (the path is the `0::` line of
    `/proc/self/cgroup`; `max` means no limit; a quota rounds up to whole
    CPUs). It is never below 1.
  - With 1 job the order and output above hold exactly, streamed. With more,
    each phase runs on a pool of at most that many workers, and the phases run
    in order: (1) format, lint, check, and `steps`; (2) import, alone; (3)
    analyzer, every test process, `test-commands`, and smoke. Within a phase
    the config order is the schedule. Check runs in a thread.
  - A step in a parallel phase has its output captured and printed as one
    block when it finishes: `== NAME`, the output, then `== NAME passed in Xs`
    or `== NAME failed after Xs`. On the first failure in a phase no new step
    starts, running steps finish and print, every failed step is named in a
    `== failed: NAME, ...` line, and the gate exits 1 without running later
    phases.
  - Each step in phase (3), a Godot process or a test command, gets a fresh
    user-data directory, removed when it finishes. `XDG_DATA_HOME` names it on
    every platform; on macOS `HOME` names it too, because Godot's user
    directory there follows `HOME` (`CFFIXED_USER_HOME` and `XDG_DATA_HOME` do
    not move it). Serial runs keep the inherited environment.
  - SIGINT and SIGTERM stop the gate: every child process session is killed,
    no new child starts, and the gate exits with 128 plus the signal number.
  - Sharded test entries: `{script: res://..., shards: N}` (N a whole number
    of at least 1) runs N processes, the I-th with `-- --shard=I/N` after its
    `--script` arguments, for I from 1 to N. Each is a step named `tests
    res://... [I/N]`. Splitting the tests is the runner's job; the runner reads
    the argument with `OS.get_cmdline_user_args()`. N = 1 is still a sharded
    entry: one process with `-- --shard=1/1`. Only a plain string entry, which
    has no `shards` key, passes no argument.
  - The analyzer step loads every scanned script headless, as hexstead's
    `tools/lint_gdscript.gd` does. Ship that script as package data and
    run it without writing into the repository; verify how Godot 4.7.1
    accepts a script outside `res://` (an absolute `--script` path, or a copy
    under `.godot/`, which git ignores). Pass the scanned roots to it. A root
    may be a directory or a `.gd` file; a root that does not exist, or roots
    that yield no scripts in total, fail the step.
- `godot-guard init`: scaffold a repo: `.godot-guard.yml` (defaults with the
  detected test script if `tests/run_tests.gd` exists), the two hooks, the
  CI workflow, and a `tools/requirements-lint.txt` line. Never overwrite an
  existing file; print what it skipped.
- `godot-guard complexity --json FILE...`: the complexity scores pedro-best-practices
  reads, byte for byte what its `complexity_gdscript.py` prints except for the
  names (godot-guard's, as in [RULES.md](RULES.md)). Ignores the config,
  excludes, and baseline, and needs no root: it scores exactly the given files,
  which may be anywhere. One compact JSON object (`json.dumps`, no trailing
  newline) keyed by each path as given, in argument order (a repeated path
  keeps its first place): a list of `{"name", "line", "end_line",
  "cyclomatic", "cognitive"}` in source order, `cognitive` null for nested
  functions, or `{"error": message}` for a file that doesn't read as UTF-8 or
  parse. Exit 0, errors included; `--json` and at least one file are required,
  else exit 2.
- `godot-guard --version`.

Godot resolution: `$GODOT`, then `godot`, `godot4` on PATH, then
`/Applications/Godot.app/Contents/MacOS/Godot`. gdtoolkit tools are the
`gdformat`/`gdlint` next to the running interpreter (`sys.executable`'s
directory), then PATH.

## Templates (package data, written by `init`)

`.githooks/pre-commit`:

```sh
#!/bin/sh
# Installed by godot-guard init. Enable once per clone: git config core.hooksPath .githooks
set -eu
guard=.venv/bin/godot-guard
[ -x "$guard" ] || guard=godot-guard
# host-check refuses nested calls, so a hook already inside its job runs directly.
if [ "${HOST_CHECK_ACTIVE:-}" != 1 ] && command -v host-check >/dev/null 2>&1; then
  exec host-check "$guard" gate --fast --staged
fi
exec "$guard" gate --fast --staged
```

host-check sets `HOST_CHECK_ACTIVE=1` for its job and refuses to nest, so a
commit or push that already runs inside a host-check job calls the guard
directly. `.githooks/pre-push`: the same with `gate` (full). `init` runs
`git config core.hooksPath .githooks` and says so.

`.github/workflows/quality.yml`: checkout, setup-python 3.12,
`pip install -r tools/requirements-lint.txt`,
`chickensoft-games/setup-godot@v2` with the version read from
`project.godot`'s `config/features` (fallback 4.7.1), `godot-guard gate`.
Match hexstead's current workflow (timeouts, `use-dotnet: false`,
`include-templates: false`).

## Tests (`tests/`, `python3 -m unittest`)

- Each rule: fixture `.gd` files covering pass, fail, and edge cases (else-if
  chains, `match` with wildcard and guard, lambdas, setters/getters, mixed
  `and`/`or`, parenthesized nested ternaries, header comments, `##` docs,
  Resource exclusion through a two-level `class_name` chain).
- Baseline: new violation, worsened, stale, loose, missing function,
  `baseline` tightening, `--init` refusal, subset checking.
- Config: defaults, unknown key, rule off, warn level, `gate.jobs` and
  sharded `gate.tests` entries (including their error messages).
- Jobs: cgroup v2 CPU budget from a fake root (rounding, tightest ancestor,
  `max`, never below 1).
- Schedule: phase order, at most `jobs` steps at once (a barrier proves
  overlap), framed output blocks, fail-fast, Godot isolation environment and
  its cleanup, and a SIGTERM to a running gate that leaves no child alive.
- CLI: `--jobs` parsing and its override of `gate.jobs`.
- `gate --fast --staged` in a temp git repo with a fake gdformat/gdlint
  (PATH shims), and `run` with a fake Godot that prints `WARNING:` and exits 0.
- Parity (skipped unless `GODOT_GUARD_PARITY_REPOS` lists repo paths): for
  every `.gd` file in each repo, godot-guard's per-function scores equal
  `complexity_gdscript.py`'s, and interface totals equal
  `godot_interface.py`'s.

## Out of scope

Scene (`.tscn`) checks, shader lint, a GUT/gdUnit migration, auto-fixing,
publishing to PyPI.
