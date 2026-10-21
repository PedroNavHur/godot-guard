# godot-guard

Guardrails for Godot 4 projects: the format, lint, typed-analyzer, complexity,
comment, and interface checks a Next.js repository gets from oxfmt, oxlint,
tsc, Husky, and CI, with a ratchet baseline so existing code can adopt them.
[SPEC.md](SPEC.md) is the full contract.

| Next.js guardrail | Godot equivalent | Owner |
| --- | --- | --- |
| oxfmt / `format:check` | `gdformat --check` | gdtoolkit |
| oxlint core rules | `gdlint` with `.gdlintrc` | gdtoolkit |
| `tsc` strict, `no-unsafe-*` | analyzer warnings set to error in `project.godot`, every script loaded headless | Godot |
| `strict: true` in tsconfig | `analyzer-config`: `project.godot` keeps those warnings at error | godot-guard |
| `complexity` 16, `cognitive-complexity` 8 | `cyclomatic-complexity`, `cognitive-complexity` | godot-guard |
| `max-lines` | none: fleet's `file-size` check caps every file at 20,000 bytes, in any language | fleet |
| `no-long-comment` | `long-comment` | godot-guard |
| prop cap | `interface-cap`: `@export`s, signals, `_init`/`setup`/`configure` parameters | godot-guard |
| `no-nested-ternary` (advisory) | `nested-ternary` (warning, never fails) | godot-guard |
| import-boundary rules | `boundary`: forbidden patterns per directory | godot-guard |
| vitest | the repository's headless test scripts | repository |
| Husky + lint-staged | `.githooks/` via `core.hooksPath` | godot-guard templates |
| verify.yml | `.github/workflows/quality.yml` | godot-guard templates |

## Install

In the game's `tools/requirements-lint.txt`, next to gdtoolkit:

```
godot-guard @ git+https://github.com/PedroNavHur/godot-guard@v0.4.0
```

Then `python3 -m venv .venv && .venv/bin/pip install -r tools/requirements-lint.txt`.
Python 3.9 or newer; Godot is found through `$GODOT`, `godot` or `godot4` on
PATH, or `/Applications/Godot.app`.

## Commands

```
godot-guard init                    # config, hooks, CI workflow, requirement line; never overwrites
godot-guard check [FILE...]         # the rules, against the baseline
godot-guard check --json [FILE...]  # the same verdict as one JSON object, for tools
godot-guard baseline [--init [--force]]
godot-guard format [FILE...]        # gdformat in write mode
godot-guard run -- ARGS...          # Godot; fails on ERROR:, WARNING:, SCRIPT ERROR: lines
godot-guard gate [--fast] [--staged | FILE...] [--jobs N|auto]
```

A directory argument stands for the scanned scripts below it. Hidden
directories and `exclude` matches are never scanned, even when named as an
argument; such an argument prints a `skipping` note and selects nothing. An
argument is judged by its path as written, so a symlink in a hidden or
excluded directory stays skipped wherever it points.
`gate --fast --staged` is the pre-commit hook: gdformat, gdlint, and `check` on
the staged content of staged scripts (exported from the git index, so unstaged
edits to them neither hide nor cause failures). Symlinked scripts are not
supported there: a staged one fails the gate, so commit the real file. The
Resource exclusion reads the other scripts in the index from the working tree,
and the staged scripts' declarations win. `gate` is the pre-push hook and CI:
format, lint, check, extra steps, editor import, analyzer, tests, and a smoke
run, timed per step.

Output lines are `path:line: message (rule)`.

### Parallel gate

`gate.jobs` (or `--jobs N|auto`, which wins for one run) sets how many gate
steps run at once. `auto` is the number of CPUs this process may use, lowered
to the tightest cgroup v2 CPU quota it runs under. With one job the gate runs
exactly as above: one step at a time, output streamed. With more, it runs in
three phases, each on at most that many workers:

1. format, lint, check, and the `steps` commands;
2. the editor import, alone;
3. the analyzer, every test process (shards included), and smoke.

Each Godot process in a parallel phase gets a fresh user-data directory, removed
afterwards, so concurrent runs do not share state. A step's output is held and
printed as one block when it finishes, framed by `== NAME` and then
`== NAME passed in Xs` or `== NAME failed after Xs`. On the first failure in a
phase no new step starts; running ones finish and print; every failed step is
listed, and later phases are skipped. Ctrl-C or SIGTERM kills the child
processes and exits 128 plus the signal number.

### Sharded tests

A `gate.tests` entry is a script path, or a mapping with `script` and `shards`:

```yaml
gate:
  jobs: auto
  tests:
    - res://tests/run_unit.gd
    - {script: res://tests/run_scenes.gd, shards: 4}
```

A mapping entry with `shards` runs one Godot process per shard, each with
`-- --shard=I/N` after its script (I from 1 to N). That holds at `shards: 1`
too, which passes `-- --shard=1/1`; a plain string entry passes no argument.
Its steps are named like
`tests res://tests/run_scenes.gd [2/4]`. The package does not split the tests:
the runner must read the argument and run only its share. A runner that ignores
it runs every test N times. A runner reads the argument with
`OS.get_cmdline_user_args()` and runs its share. Here the `test_*` methods are
the runner's own and return true on pass:

```gdscript
extends SceneTree

func _init() -> void:
	var shard := 1
	var shards := 1
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--shard="):
			var parts := arg.trim_prefix("--shard=").split("/")
			shard = int(parts[0])
			shards = int(parts[1])
	var cases := ["test_movement", "test_combat", "test_inventory", "test_save"]
	var failed := 0
	for i in cases.size():
		if i % shards == shard - 1 and not call(cases[i]):
			failed += 1
	quit(failed)
```

Unknown keys, a non-string `script`, and a `shards` that is not a whole number
of at least 1 are config errors.

`check --json` prints one JSON object instead: `version` (1), `root`, the
effective `limits` and rule `levels`, the `files` it checked, and `problems`,
each with `path`, `line`, `rule`, `level` (`error` or `warn`), and `message`.
It exits as `check` does, and `skipping` notes stay on stderr. `version` rises
only when a key is removed or changes meaning; new keys can appear without a
bump. [SPEC.md](SPEC.md) has the details.

## Config: `.godot-guard.yml`

Every key is optional; the values below are the defaults, except the boundary
example (the default is `[]`). Unknown and repeated keys are errors, and so are YAML
anchors, aliases, and `<<` merge keys; an omitted key or null
takes the default, and a value of the wrong type is an error.

```yaml
paths: [.]
exclude: [addons/**, .godot/**]
limits: {cognitive: 8, cyclomatic: 16, comment-lines: 2, inputs: 8}
rules:          # error, warn, or off
  cognitive-complexity: error
  cyclomatic-complexity: error
  long-comment: error
  interface-cap: error
  nested-ternary: warn
  analyzer-config: error
relaxed-warnings: []  # analyzer warnings project.godot may keep below error
boundaries:
  - name: simulation-domain
    paths: [scripts/simulation/**]
    forbid: ['\bSprite3D\b', 'res://scenes/']
    message: simulation code must not reference presentation
gate:
  format: true
  lint: true
  steps: []                  # shell commands run from the repository root
  import: true
  analyzer: true
  jobs: 1                    # workers: a whole number, or auto
  tests: []                  # res:// scripts, or {script: res://..., shards: N}
  smoke: [--quit-after, "3"] # [] or false skips it
```

[examples/](examples/) holds the hexstead and wingmen configurations.

## Baseline workflow

`.godot-guard-baseline.json` records the items over a limit when a repository
adopts godot-guard; it may only shrink.

1. Adoption: `godot-guard baseline --init` records every current violation.
   Commit it with the config.
2. A recorded item may not grow. Anything new over a limit fails `check`.
3. When an item improves, `check` fails until the baseline follows: run
   `godot-guard baseline` to lower improved entries and drop fixed or deleted
   ones. It never adds or raises an entry.
4. Raising a limit or re-baselining on purpose is `baseline --init --force`,
   reviewed like any other change.

Keys are the repo-relative path for file rules (`long-comment`,
`interface-cap`) and `path::name` for the complexity rules, where the name is
qualified with `.`: `Inner.method`, `speed.set`, and `outer.<lambda#2>` for the
second anonymous lambda in `outer`. A name repeated in one file is numbered on
every occurrence (`f#1`, `f#2`).

These keys are positional, so inserting, removing, or reordering a lambda (or a
same-named one) ahead of a baselined lambda renames its key, and `check` reports
the old entry as stale and the lambda as a new violation. Fix it by refactoring
the lambda below the limit, or by renaming the entry's key in the baseline by
hand.

Boundary hits, nested ternaries, and `analyzer-config` are never baselined.

## Development

```
python3 -m venv .venv && .venv/bin/pip install -e .
.venv/bin/python -m unittest
GODOT_GUARD_PARITY_REPOS=/path/to/hexstead:/path/to/wingmen .venv/bin/python -m unittest tests.test_parity
```

The parity tests compare the scores with pedro-best-practices' reference
scripts (`GODOT_GUARD_PBP_SCRIPTS` overrides their location).
