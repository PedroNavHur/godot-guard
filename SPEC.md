# godot-guard v0.1 spec

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
| `complexity` max 16, `cognitive-complexity` max 8 | `cognitive-complexity`, `cyclomatic-complexity` | godot-guard |
| `max-lines` 300 (skip blank and comment lines) | `file-lines` | godot-guard |
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
  file-lines: 300         # code lines: blank and comment-only lines skipped
  comment-lines: 2        # prose lines per comment run
  inputs: 8               # interface cap
rules:                    # set a rule to off to disable it
  cognitive-complexity: error
  cyclomatic-complexity: error
  file-lines: error
  long-comment: error
  interface-cap: error
  nested-ternary: warn    # Pedro's rule: advisory, never a merge blocker
boundaries: []            # see "boundary"
gate:
  format: true            # gdformat --check <paths>
  lint: true              # gdlint <paths>
  steps: []               # extra shell commands, run from the repo root, before Godot
  import: true            # godot --headless --editor --quit
  analyzer: true          # load every scanned script headless
  tests: []               # res:// scripts run with --script, in order
  smoke: [--quit-after, "3"]   # args for a main-scene run; [] or false skips it
```

Unknown keys are errors (catch typos), and so is a key repeated in one
mapping at any depth (reported with its line). An omitted key or an explicit null
takes its default; any other value of the wrong type (a top-level `false` or
`[]`, `limits: false`, `paths: {}`) is an error naming the key and the
expected type. Rule levels: `error`, `warn`, `off`.

## Rules

All rules run on the parsed tree from `gdtoolkit.parser` where a tree is
needed; a file that fails to parse is one `parse-error` error (gdlint reports
the details).

- **cognitive-complexity / cyclomatic-complexity**: per function, scored as in
  `complexity_gdscript.py` (SonarSource cognitive v1.7, ESLint classic
  cyclomatic). Nested lambdas fold into the enclosing function for cognitive,
  score their own cyclomatic. Key: `path::Qualified.name`, unique per
  function. Names join with `.` only: inner classes (`Inner.method`),
  property accessors (`speed.set`, `speed.get`), and lambdas, which take the
  enclosing function's name and an ordinal among its anonymous lambdas
  (`outer.<lambda#2>`; `<lambda#1>` or `Inner.<lambda#1>` outside any
  function; `outer.named` for a named lambda). A name still repeated in one
  file gets `#N` on every occurrence (`f#1`, `f#2`).
- **file-lines**: lines that are neither blank nor comment-only (`#` or `##`).
  Key: `path`.
- **long-comment**: an unbroken run of own-line `#` comments with more than
  `comment-lines` prose lines. Exempt: `##` doc comments, trailing comments,
  the file header block above the first line of code (after
  `extends`/`class_name`/`@tool`/`@icon`), and non-prose lines (blank,
  separators, a lone URL or path reference, ticket IDs, `Run:` lines,
  `gdlint:`/`warning_ignore`/`region` directives). Match hexstead's
  `tools/check_gdscript.py` and pedro-best-practices `long_comments.py`;
  where they disagree, follow `long_comments.py`. Key: `path` (the count of
  long comments in the file).
- **interface-cap**: top-level `@export` vars (not group/subgroup/category),
  `signal`s, and parameters of `_init`, `setup`, `configure`. Scripts whose
  `extends` chain reaches `Resource` through the repo's `class_name`s are
  excluded, so the whole repo's `extends`/`class_name` map is read even when
  checking a subset. Key: `path`.
- **nested-ternary**: a conditional expression with another conditional in
  its condition or either branch, including parenthesized forms. Report the
  outer one once. Never baselined, never fails the run.
- **boundary**: config entries like

  ```yaml
  boundaries:
    - name: simulation-domain
      paths: [scripts/simulation/**, scripts/core/**]
      forbid: ['\bSprite3D\b', 'res://scenes/']
      message: simulation code must not reference presentation
  ```

  Each code line (comments stripped; a `#` inside a string, including
  `&""` and `^""` literals and triple-quoted strings, is not a comment) in a
  matching file is searched for each pattern. String contents are searched. Not baselined: existing hits must be fixed before adoption.
  hexstead's `domain-boundary` rule moves here unchanged.

## Ratchet baseline: `.godot-guard-baseline.json`

```json
{
  "version": 1,
  "cognitive-complexity": {"ui/fx.gd::draw_foliage": 33},
  "cyclomatic-complexity": {},
  "file-lines": {"scenes/battle.gd": 1500},
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
  every scanned file), against the baseline. Exit 1 on any error; warnings
  print but exit 0. End with a one-line summary: errors, warnings, files.
- `godot-guard baseline [--init [--force]]`: as above.
- `godot-guard format [FILE...]`: `gdformat` in write mode on the files or
  paths. For the pre-commit skill's format step.
- `godot-guard run -- ARGS...`: run Godot (resolved as below) with ARGS,
  stream output, and fail when Godot exits non-zero or prints a line
  containing `SCRIPT ERROR:` or starting with `ERROR:` or `WARNING:`. This is
  hexstead's `tools/check_godot_output.py`.
- `godot-guard gate [--fast] [--staged | FILE...]`:
  - `--fast` (pre-commit): `gdformat --check`, `gdlint`, and `check` on the
    selected files only. `--staged` selects staged `.gd` files that exist in
    the working tree (`git diff --cached --name-only --diff-filter=ACMR`).
    Nothing selected means nothing to do, exit 0.
  - full (default, pre-push and CI): the whole configured gate in this order,
    stopping at the first failure: format, lint, check, steps, import,
    analyzer, tests, smoke. Godot steps go through `run`. Print a header per
    step and the elapsed seconds per step.
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
- Config: defaults, unknown key, rule off, warn level.
- `gate --fast --staged` in a temp git repo with a fake gdformat/gdlint
  (PATH shims), and `run` with a fake Godot that prints `WARNING:` and exits 0.
- Parity (skipped unless `GODOT_GUARD_PARITY_REPOS` lists repo paths): for
  every `.gd` file in each repo, godot-guard's per-function scores equal
  `complexity_gdscript.py`'s, and interface totals equal
  `godot_interface.py`'s.

## Out of scope for v0.1

Scene (`.tscn`) checks, shader lint, a GUT/gdUnit migration, auto-fixing,
publishing to PyPI.
