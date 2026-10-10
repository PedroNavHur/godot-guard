"""Score GDScript functions on a gdtoolkit parse tree.

Ported from pedro-best-practices `complexity_core.py` and
`complexity_gdscript.py`; the scores must stay identical (tests/test_parity.py).

Cognitive complexity follows the SonarSource whitepaper (G. Ann Campbell,
v1.7): structural increments cost 1 plus the current nesting, fundamental
increments cost 1, nested functions add a nesting level and fold their score
into the enclosing top-level function. Cyclomatic complexity follows ESLint's
classic `complexity` rule: 1 plus one per decision point, scored for every
function on its own. Language mappings:
- `elif` is an else-if (+1 flat); `else` is +1 flat.
- `match` is one structural increment; each non-wildcard branch is a
  cyclomatic decision, and a `when` guard adds 1 to both metrics.
- `and`/`&&` and `or`/`||` are the same operators; `not` ends a chain.
- Lambdas nest like any nested function, named after the enclosing function:
  `outer.<lambda#2>` for the second anonymous one in `outer`. Property getters
  and setters score as their own functions, named `property.set` and `property.get`.
- Recursion: a call to the function's own name, or `self.` plus that name.
"""

from __future__ import annotations

from contextlib import contextmanager
from dataclasses import dataclass, field

BOOLEAN = {"or_test": "or", "asless_or_test": "or", "and_test": "and", "asless_and_test": "and"}
TERNARY = {"test_expr", "asless_test_expr"}
LOOPS = {"for_stmt", "for_stmt_typed", "while_stmt"}


@dataclass
class Function:
    name: str
    line: int
    end_line: int
    cyclomatic: int = 1
    cognitive: int | None = None  # None for nested functions: counted in the top-level one


@dataclass
class _Frame:
    function: Function
    nesting: int
    recursion_counted: bool = False
    own_names: tuple[str, ...] = ()


@dataclass
class Scorer:
    functions: list[Function] = field(default_factory=list)
    _frames: list[_Frame] = field(default_factory=list)

    @property
    def active(self) -> bool:
        return bool(self._frames)

    @contextmanager
    def function(self, name: str, line: int, end_line: int, own_names: tuple[str, ...] = ()):
        """Score a function body. `own_names` are the call forms that count as recursion."""
        function = Function(name, line, end_line)
        if self._frames:
            nesting = self._frames[-1].nesting + 1
        else:
            nesting = 0
            function.cognitive = 0
        self.functions.append(function)
        self._frames.append(_Frame(function, nesting, own_names=own_names))
        try:
            yield function
        finally:
            self._frames.pop()

    @contextmanager
    def nested(self):
        """A nesting region: the body of an if, else, loop, case, or ternary branch."""
        if not self._frames:
            yield
            return
        self._frames[-1].nesting += 1
        try:
            yield
        finally:
            self._frames[-1].nesting -= 1

    def enclosing(self) -> str:
        return self._frames[-1].function.name if self._frames else ""

    def _top(self) -> Function | None:
        return self._frames[0].function if self._frames else None

    def structural(self) -> None:
        """if, ternary, match, loop: 1 plus the nesting level."""
        if self._frames:
            self._top().cognitive += 1 + self._frames[-1].nesting

    def fundamental(self, amount: int = 1) -> None:
        """else, elif, operator runs, recursion: a flat 1 each."""
        if self._frames and amount:
            self._top().cognitive += amount

    def decision(self, amount: int = 1) -> None:
        """A cyclomatic decision point in the innermost function."""
        if self._frames and amount:
            self._frames[-1].function.cyclomatic += amount

    def call(self, form: str) -> None:
        """Count recursion once per function when `form` names the function itself."""
        if not self._frames:
            return
        frame = self._frames[-1]
        if not frame.recursion_counted and form in frame.own_names:
            frame.recursion_counted = True
            self.fundamental()


def operator_runs(operators: list[str]) -> int:
    """Runs of like operators in a flattened boolean chain: `a and b or c` is 2."""
    runs, previous = 0, None
    for operator in operators:
        if operator != previous:
            runs += 1
            previous = operator
    return runs


def is_tree(node) -> bool:
    return hasattr(node, "data")


def token_text(node) -> str:
    if is_tree(node):
        return ""
    return str(node)


def end_line(node) -> int:
    lines = [getattr(token, "end_line", None) or token.line for token in node.scan_values(lambda v: hasattr(v, "line"))]
    return max(lines, default=node.meta.line)


def first_name(node) -> str:
    for child in node.children:
        if not is_tree(child) and child.type == "NAME":
            return str(child)
    return "<anonymous>"


def is_wildcard(pattern) -> bool:
    return is_tree(pattern) and any(is_tree(c) and c.data == "wildcard_pattern" for c in pattern.children)


class Analyzer:
    def __init__(self):
        self.scorer = Scorer()
        self.seen_boolean: set[int] = set()
        self.classes: list[str] = []
        self.last_var = ""
        self.lambdas: dict[str, int] = {}

    def walk(self, node) -> None:
        if not is_tree(node):
            return
        handler = getattr(self, f"visit_{node.data}", None)
        if handler:
            handler(node)
        elif node.data in LOOPS:
            self.visit_loop(node)
        elif node.data in BOOLEAN:
            self.visit_boolean(node)
        elif node.data in TERNARY:
            self.visit_ternary(node)
        else:
            self.children(node)

    def children(self, node) -> None:
        for child in node.children:
            self.walk(child)

    def nested_children(self, children) -> None:
        with self.scorer.nested():
            for child in children:
                self.walk(child)

    # Functions and classes

    def visit_class_def(self, node) -> None:
        self.classes.append(first_name(node))
        self.children(node)
        self.classes.pop()

    def visit_class_var_stmt(self, node) -> None:
        self.last_var = first_name(node.children[0])
        self.children(node)

    def function(self, node, name: str, body) -> None:
        in_class = not self.scorer.active
        qualified = ".".join([*self.classes, name]) if in_class and self.classes else name
        own = (name, f"self.{name}")
        with self.scorer.function(qualified, node.meta.line, end_line(node), own):
            for child in body:
                self.walk(child)

    def visit_func_def(self, node) -> None:
        header, *body = node.children
        self.function(node, first_name(header), body)

    def visit_property_custom_setter(self, node) -> None:
        self.function(node, f"{self.last_var}.set", node.children)

    def visit_property_custom_getter(self, node) -> None:
        self.function(node, f"{self.last_var}.get", node.children)

    def visit_lambda(self, node) -> None:
        header, *body = node.children
        name = first_name(header)
        with self.scorer.function(self.lambda_name(name), node.meta.line, end_line(node), (name,)):
            for child in body:
                self.walk(child)

    def lambda_name(self, name: str) -> str:
        """`outer.name`, or `outer.<lambda#N>` numbered within `outer` for an anonymous lambda."""
        enclosing = self.scorer.enclosing() or ".".join(self.classes)
        if name == "<anonymous>":
            self.lambdas[enclosing] = self.lambdas.get(enclosing, 0) + 1
            name = f"<lambda#{self.lambdas[enclosing]}>"
        return ".".join(part for part in (enclosing, name) if part)

    def visit_standalone_call(self, node) -> None:
        self.scorer.call(first_name(node))
        self.children(node)

    def visit_getattr_call(self, node) -> None:
        target = node.children[0]
        if is_tree(target) and target.data == "getattr":
            names = [str(t) for t in target.children if not is_tree(t) and t.type == "NAME"]
            if len(names) == 2:
                self.scorer.call(".".join(names))
        self.children(node)

    # Branches

    def visit_if_stmt(self, node) -> None:
        for branch in node.children:
            if not is_tree(branch):
                continue
            if branch.data == "if_branch":
                self.scorer.structural()
            else:
                self.scorer.fundamental()
            if branch.data == "else_branch":
                self.nested_children(branch.children)
                continue
            self.scorer.decision()
            condition, *body = branch.children
            self.walk(condition)
            self.nested_children(body)

    def visit_loop(self, node) -> None:
        self.scorer.structural()
        self.scorer.decision()
        children = [child for child in node.children if is_tree(child)]
        header = next((i for i, child in enumerate(children) if child.data == "expr"), 0)
        for child in children[: header + 1]:
            self.walk(child)
        self.nested_children(children[header + 1 :])

    def visit_match_stmt(self, node) -> None:
        self.scorer.structural()
        subject, *branches = node.children
        self.walk(subject)
        for branch in branches:
            if is_tree(branch):
                self.match_branch(branch)

    def match_branch(self, branch) -> None:
        pattern, *rest = branch.children
        wildcard = is_wildcard(pattern)
        if branch.data == "guarded_match_branch":
            guard, *rest = rest
            self.scorer.decision(1 if wildcard else 2)
            self.scorer.fundamental()
            self.walk(guard)
        elif not wildcard:
            self.scorer.decision()
        self.nested_children(rest)

    # Expressions

    def visit_ternary(self, node) -> None:
        parts = [child for child in node.children if is_tree(child) or token_text(child) not in ("if", "else")]
        if len(parts) != 3:
            self.children(node)
            return
        value, condition, alternative = parts
        self.scorer.structural()
        self.scorer.decision()
        self.walk(condition)
        self.nested_children([value, alternative])

    def flatten(self, node, operators: list[str]) -> None:
        if not (is_tree(node) and node.data in BOOLEAN):
            return
        self.seen_boolean.add(id(node))
        for child in node.children:
            if is_tree(child):
                self.flatten(child, operators)
            elif token_text(child) in ("or", "||"):
                operators.append("or")
            elif token_text(child) in ("and", "&&"):
                operators.append("and")

    def visit_boolean(self, node) -> None:
        own = [child for child in node.children if not is_tree(child) and token_text(child) in ("or", "||", "and", "&&")]
        if id(node) not in self.seen_boolean:
            operators: list[str] = []
            self.flatten(node, operators)
            self.scorer.fundamental(operator_runs(operators))
        self.scorer.decision(len(own))
        self.children(node)


def score(tree) -> list[Function]:
    analyzer = Analyzer()
    analyzer.walk(tree)
    return analyzer.scorer.functions


def nested_ternaries(tree) -> list[int]:
    """Lines of each outermost conditional expression that holds another one."""
    found: list[int] = []
    pending = [tree]
    while pending:
        node = pending.pop()
        if node.data not in TERNARY:
            pending.extend(child for child in node.children if is_tree(child))
            continue
        inner = (child for child in node.iter_subtrees() if child is not node and child.data in TERNARY)
        if next(inner, None) is not None:
            found.append(node.meta.line)
    return sorted(found)
