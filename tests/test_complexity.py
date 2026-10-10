from __future__ import annotations

import unittest

from gdtoolkit.parser import parser

from godot_guard import complexity

from . import fixtures


def tree(source: str):
    return parser.parse(source, gather_metadata=True)


class ComplexityTest(unittest.TestCase):
    def test_scores_every_function(self):
        scores = {f.name: (f.line, f.cognitive, f.cyclomatic) for f in complexity.score(tree(fixtures.COMPLEXITY))}
        self.assertEqual(scores, fixtures.COMPLEXITY_SCORES)

    def test_lambda_folds_into_enclosing_cognitive(self):
        source = "func f(x):\n\tvar g = func(y):\n\t\tif y:\n\t\t\treturn 1\n\t\treturn 0\n\treturn g\n"
        functions = {f.name: f for f in complexity.score(tree(source))}
        self.assertEqual(functions["f"].cognitive, 2)
        self.assertEqual(functions["<lambda>"].cyclomatic, 2)

    def test_nested_ternary_reports_outermost_once(self):
        self.assertEqual(complexity.nested_ternaries(tree(fixtures.NESTED_TERNARY)), [6, 7, 8])

    def test_flat_ternaries_are_not_nested(self):
        source = "func f(a, b):\n\tvar x = 1 if a else 2\n\tvar y = 3 if b else 4\n"
        self.assertEqual(complexity.nested_ternaries(tree(source)), [])


if __name__ == "__main__":
    unittest.main()
