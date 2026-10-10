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
| `complexity` 16, `cognitive-complexity` 8 | `cyclomatic-complexity`, `cognitive-complexity` | godot-guard |
| `max-lines` 300 | `file-lines` (blank and comment lines skipped) | godot-guard |
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
godot-guard @ git+https://github.com/PedroNavHur/godot-guard@v0.1.0
```

Then `python3 -m venv .venv && .venv/bin/pip install -r tools/requirements-lint.txt`.
Python 3.9 or newer; Godot is found through `$GODOT`, `godot` or `godot4` on
PATH, or `/Applications/Godot.app`.

## Commands

```
godot-guard init                    # config, hooks, CI workflow, requirement line; never overwrites
godot-guard check [FILE...]         # the rules, against the baseline
godot-guard baseline [--init [--force]]
godot-guard format [FILE...]        # gdformat in write mode
godot-guard run -- ARGS...          # Godot; fails on ERROR:, WARNING:, SCRIPT ERROR: lines
godot-guard gate [--fast] [--staged | FILE...]
```

A directory argument stands for the scanned scripts below it.
`gate --fast --staged` is the pre-commit hook: gdformat, gdlint, and `check` on
the staged content of staged scripts (exported from the git index, so unstaged
edits neither hide nor cause failures). `gate` is the pre-push hook and CI: format, lint, check, extra
steps, editor import, analyzer, tests, and a smoke run, timed per step.

Output lines are `path:line: message (rule)`.

## Config: `.godot-guard.yml`

Every key is optional; the values below are the defaults, except the boundary
example (the default is `[]`). Unknown and repeated keys are errors; an omitted key or null
takes the default, and a value of the wrong type is an error.

```yaml
paths: [.]
exclude: [addons/**, .godot/**]
limits: {cognitive: 8, cyclomatic: 16, file-lines: 300, comment-lines: 2, inputs: 8}
rules:          # error, warn, or off
  cognitive-complexity: error
  cyclomatic-complexity: error
  file-lines: error
  long-comment: error
  interface-cap: error
  nested-ternary: warn
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
  tests: []                  # res:// scripts run with --script
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

Keys are the repo-relative path for file rules (`file-lines`, `long-comment`,
`interface-cap`) and `path::name` for the complexity rules, where the name is
qualified with `.`: `Inner.method`, `speed.set`, and `outer.<lambda#2>` for the
second anonymous lambda in `outer`. A name repeated in one file is numbered on
every occurrence (`f#1`, `f#2`).

Boundary hits and nested ternaries are never baselined.

## Development

```
python3 -m venv .venv && .venv/bin/pip install -e .
.venv/bin/python -m unittest
GODOT_GUARD_PARITY_REPOS=/path/to/hexstead:/path/to/wingmen .venv/bin/python -m unittest tests.test_parity
```

The parity tests compare the scores with pedro-best-practices' reference
scripts (`GODOT_GUARD_PBP_SCRIPTS` overrides their location).
