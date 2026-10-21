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
- **analyzer-config**: `project.godot` sets each GDScript analyzer warning in
  `godot_warnings.REQUIRED` to 2 (error) in its `[debug]` section. The
  analyzer step fails only at error: loaded headless, Godot 4.7 prints
  nothing for a warning at 1, so a warning left there is never checked. The
  set is hexstead's 43 plus the four Godot defaults to error
  (`get_node_default_without_onready`, `inference_on_variant`,
  `native_method_override`, `onready_with_export`), which may not be lowered;
  a setting the file omits takes Godot's default. It reports each required
  setting below error on its line, one problem on the `[debug]` header (line
  1 without one) listing the omitted ones whose default is below error, and
  `gdscript/warnings/enable` set to anything but `true`. `relaxed-warnings`
  names warnings it skips; a name outside the set is a config error. It runs
  only when `check` covers the whole project, never on named files or the
  staged gate, and is never baselined.

### Pattern rules

`patterns.py` matches these on the parse tree. A *hot function* is
`_process`, `_physics_process`, `_input`, `_unhandled_input`,
`_unhandled_key_input`, `_gui_input`, or `_shortcut_input`, in the script or
an inner class; the *frame functions* are the first two. Lambdas inside a hot
function are skipped, since they run when called, and scripts that extend
`SceneTree` or `MainLoop` (test and tool runners) are exempt from the
hot-path rules. Only code written in the hot function counts, not the
helpers it calls.

The four counted rules are baselined like long-comment: one item per file
whose value is its count of hits, key `path`, limit 0, each hit reported on
its own line. A file that fails to parse keeps its entries.

- **lookup-in-hot-path**: `$path`, `%name`, or a call to `get_node`,
  `get_node_or_null`, `find_child`, `find_children`, `find_parent`,
  `get_nodes_in_group`, or `get_first_node_in_group` (on any receiver) in a
  hot function. pedro-best-practices `godot/lookup-in-hot-path`.
- **load-in-hot-path**: a `load()` call, bare or on a receiver such as
  `ResourceLoader`, in a hot function. `preload` is resolved at compile time
  and passes. pedro-best-practices `godot/load-in-hot-path`.
- **private-export**: a class variable named with a leading `_` under an
  `@export` annotation of any kind except group, subgroup, and category.
- **self-assign**: `x = x`, the two sides equal as parse trees (`self.a =
  self.a` too). Compound assignment (`x += x`) passes.
- **per-frame-allocation**: an array or dictionary literal or a `.new()` call
  in a frame function. Advisory like nested-ternary: a warning, never
  baselined, never a failure; `error` is a config error.
  pedro-best-practices `godot/per-frame-allocation`.
