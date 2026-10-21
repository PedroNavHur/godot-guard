"""Syntax-pattern rules on a gdtoolkit parse tree: hot-path work, private exports, and self-assignment.

A hot function is a callback Godot runs every frame or input event; a frame function, every frame.
Lambdas inside one are skipped, since they run when called. SceneTree and MainLoop scripts are
test and tool runners, not game code, and are exempt from the hot-path rules.
"""

from __future__ import annotations

from .complexity import first_name, is_tree
from .interface import is_export_annotation

FRAME_FUNCTIONS = frozenset({"_process", "_physics_process"})
HOT_FUNCTIONS = FRAME_FUNCTIONS | {"_input", "_unhandled_input", "_unhandled_key_input", "_gui_input", "_shortcut_input"}
LOOKUP_CALLS = frozenset({
    "get_node", "get_node_or_null", "find_child", "find_children", "find_parent", "get_nodes_in_group",
    "get_first_node_in_group",
})
NODE_PATHS = {"get_node": "$path", "unique_node_path": "%name"}
LITERALS = {"array": "an array literal", "dict": "a dictionary literal"}
RUNNERS = frozenset({"SceneTree", "MainLoop"})
EXTENDS_STATEMENTS = frozenset({"extends_stmt", "classname_extends_stmt"})
HOT_RULES = ("lookup-in-hot-path", "load-in-hot-path", "per-frame-allocation")
# Hidden from the inspector, so a private name is their purpose.
STORAGE_EXPORTS = frozenset({"export_storage"})

# Baselined per file, counted like long-comment: rule -> the noun its count labels.
COUNTED = {
    "lookup-in-hot-path": "hot-path lookups",
    "load-in-hot-path": "hot-path loads",
    "private-export": "private exports",
    "self-assign": "self-assignments",
}
ADVISORY = ("per-frame-allocation",)

Hits = dict[str, list[tuple[int, str]]]


def called_name(node) -> str:
    """The function or method a call node names: `f` in `f()` and `a.b.f()`; "" for anything else."""
    if node.data == "standalone_call":
        return first_name(node)
    target = node.children[0] if node.data == "getattr_call" else None
    if not (is_tree(target) and target.data == "getattr"):
        return ""
    names = [str(child) for child in target.children if not is_tree(child) and child.type == "NAME"]
    return names[-1] if names else ""


def hot_hit(node, function: str) -> tuple[str, str] | None:
    """(rule, message) for work a hot function should not repeat each call."""
    called = called_name(node)
    if node.data in NODE_PATHS or called in LOOKUP_CALLS:
        what = NODE_PATHS.get(node.data) or f"{called}()"
        return "lookup-in-hot-path", f"{what} inside {function}(): resolve it once into an @onready var or in _ready()"
    if called == "load":
        return "load-in-hot-path", f"load() inside {function}() reads the resource each call: preload it or load it once"
    what = LITERALS.get(node.data) or (".new()" if called == "new" else "")
    if what and function in FRAME_FUNCTIONS:
        return "per-frame-allocation", f"{what} inside {function}() allocates every frame; reuse one if it does not change"
    return None


def subtrees(node) -> list:
    """The child subtrees of `node`; a `$%name` is one lookup, so its path child is skipped."""
    if node.data == "get_node":
        return []
    return [child for child in node.children if is_tree(child)]


def body_nodes(node):
    """Every subtree below `node`, without descending into lambdas."""
    pending = subtrees(node)
    while pending:
        current = pending.pop()
        if current.data == "lambda":
            continue
        yield current
        pending.extend(subtrees(current))


def hot_functions(tree) -> list[tuple[str, object]]:
    """(name, func_def) for each hot function; none in a runner script."""
    if extends(tree) in RUNNERS:
        return []
    named = ((first_name(function.children[0]), function) for function in tree.find_data("func_def"))
    return [(name, function) for name, function in named if name in HOT_FUNCTIONS]


def hot_hits(tree) -> Hits:
    found: Hits = {rule: [] for rule in HOT_RULES}
    for name, function in hot_functions(tree):
        for node in body_nodes(function):
            hit = hot_hit(node, name)
            if hit:
                found[hit[0]].append((node.meta.line, hit[1]))
    return found


def is_export(annotation) -> bool:
    name = str(annotation.children[0]) if annotation.children else ""
    return is_export_annotation(name) and name not in STORAGE_EXPORTS


def exported_variables(body) -> list:
    """`class_var_stmt` nodes in one class body that carry an @export annotation."""
    exported, found = False, []
    for child in (c for c in body.children if is_tree(c)):
        if child.data == "annotation":
            exported = exported or is_export(child)
            continue
        if exported and child.data == "class_var_stmt":
            found.append(child)
        exported = False
    return found


def private_exports(tree) -> list[tuple[int, str]]:
    variables = [v for body in (tree, *tree.find_data("class_def")) for v in exported_variables(body)]
    names = [(v.meta.line, first_name(v.children[0])) for v in variables]
    return [(line, f"@export on {name}: the underscore marks it private; drop one of them")
            for line, name in names if name.startswith("_")]


def source_text(node) -> str:
    if not is_tree(node):
        return str(node)
    return "".join(str(token) for token in node.scan_values(lambda value: not is_tree(value)))


def self_assigns(tree) -> list[tuple[int, str]]:
    found = []
    for node in tree.find_data("assnmnt_expr"):
        if len(node.children) == 3 and str(node.children[1]) == "=" and node.children[0] == node.children[2]:
            found.append((node.meta.line, f"{source_text(node.children[0])} is assigned to itself"))
    return found


def extends(tree) -> str:
    """The base in the script's own `extends` or `class_name X extends` line; "" without one."""
    statement = next((c for c in tree.children if is_tree(c) and c.data in EXTENDS_STATEMENTS), None)
    return source_text(statement.children[-1]) if statement is not None else ""


def scan(tree) -> Hits:
    """(line, message) hits by rule, for every rule in COUNTED and ADVISORY, sorted by line."""
    found = hot_hits(tree)
    found["private-export"] = private_exports(tree)
    found["self-assign"] = self_assigns(tree)
    return {rule: sorted(hits) for rule, hits in found.items()}
