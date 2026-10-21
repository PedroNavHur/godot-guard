# godot-guard rules

The rules `check` runs, part of [SPEC.md](SPEC.md).

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
