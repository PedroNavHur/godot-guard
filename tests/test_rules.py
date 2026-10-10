from __future__ import annotations

import unittest

from godot_guard import comments, interface
from godot_guard.rules import Surfaces, analyze

from . import fixtures
from .helpers import Repo


class CommentTest(unittest.TestCase):
    def test_only_the_long_run_after_the_header(self):
        found = comments.long_comments(fixtures.COMMENTS, 2)
        self.assertEqual([(c.line, c.prose) for c in found], [(25, 3)])

    def test_cap_is_configurable(self):
        self.assertEqual(comments.long_comments(fixtures.COMMENTS, 3), [])

    def test_doc_comments_break_a_run(self):
        source = "extends Node\nvar a := 1\n# one\n# two\n## doc\n# three\n"
        self.assertEqual(comments.long_comments(source, 2), [])

    def test_header_ends_at_the_first_code_line(self):
        source = "extends Node\n## Class doc.\nvar a := 1\n# one\n# two\n# three\n"
        self.assertEqual([c.line for c in comments.long_comments(source, 2)], [4])

    def test_license_text_is_exempt(self):
        source = "var a := 1\n# Copyright (c) 2026 Someone\n# Permission is granted\n# to anyone at all\n"
        self.assertEqual(comments.long_comments(source, 2), [])

    def test_code_lines_skip_blank_and_comment_lines(self):
        self.assertEqual(comments.code_lines(fixtures.COMMENTS), 7)


class InterfaceTest(unittest.TestCase):
    def test_counts_exports_signals_and_setup_parameters(self):
        surface = interface.parse("wide.gd", fixtures.WIDE_NODE)
        self.assertEqual(surface.exports, ["color", "size", "level", "late"])
        self.assertEqual(surface.signals, ["opened", "closed"])
        self.assertEqual(surface.parameters, {"_init": ["a", "b"], "setup": ["first", "second", "third"]})
        self.assertEqual(surface.total, 11)

    def test_resource_chain_through_two_class_names_is_excluded(self):
        repo = Repo({
            "data/item.gd": fixtures.RESOURCE_BASE,
            "data/weapon.gd": fixtures.RESOURCE_CHILD,
            "data/sword.gd": fixtures.RESOURCE_GRANDCHILD,
            "ui/wide.gd": fixtures.WIDE_NODE,
        })
        self.addCleanup(repo.cleanup)
        config = repo.config()
        surfaces = Surfaces(repo.root)
        sword = analyze(config, "data/sword.gd", surfaces)
        wide = analyze(config, "ui/wide.gd", surfaces)
        self.assertNotIn("interface-cap", [i.rule for i in sword.items])
        self.assertEqual([i.value for i in wide.items if i.rule == "interface-cap"], [11])

    def test_later_roots_override_and_dangling_links_are_skipped(self):
        working = Repo({"base.gd": "class_name Base\nextends Resource\n", "other.gd": "extends Node\n"})
        staged = Repo({"base.gd": "class_name Base\nextends Node\n"})
        for repo in (working, staged):
            self.addCleanup(repo.cleanup)
        (working.root / "dangling.gd").symlink_to("missing.txt")
        surfaces = Surfaces(working.root, staged.root)
        self.assertEqual(sorted(surfaces.by_path), ["base.gd", "other.gd"])
        self.assertEqual(surfaces.by_class["Base"].extends, "Node")


class BoundaryTest(unittest.TestCase):
    def test_forbidden_patterns_in_matching_paths(self):
        config_text = (
            "boundaries:\n"
            "  - name: simulation-domain\n"
            "    paths: [scripts/simulation/**]\n"
            "    forbid: ['\\bSprite3D\\b', 'res://scenes/']\n"
            "    message: simulation code must not reference presentation\n"
        )
        repo = Repo({"scripts/simulation/sim.gd": fixtures.BOUNDARY, "scripts/ui/view.gd": fixtures.BOUNDARY}, config_text)
        self.addCleanup(repo.cleanup)
        config = repo.config()
        surfaces = Surfaces(repo.root)
        hits = [p for p in analyze(config, "scripts/simulation/sim.gd", surfaces).problems if p.rule == "boundary"]
        self.assertEqual([p.line for p in hits], [4, 5])
        self.assertIn("simulation code must not reference presentation", hits[0].message)
        other = analyze(config, "scripts/ui/view.gd", surfaces).problems
        self.assertEqual([p for p in other if p.rule == "boundary"], [])

    def test_hash_inside_a_string_is_not_a_comment(self):
        source = (
            'extends Node\n\n\nfunc f():\n\tvar a = get_node("#") as Sprite3D\n'
            "\tvar b = '#' # Sprite3D in a comment\n\tvar c = get_node(&\"x#\") as Sprite3D\n"
            '\tvar d = "\\"#" + str(Sprite3D)\n\tvar e = """\n# text\n"""  # Sprite3D\n\treturn Sprite3D\n'
        )
        repo = Repo({"sim.gd": source}, "boundaries:\n  - name: domain\n    paths: ['**']\n    forbid: ['\\bSprite3D\\b']\n")
        self.addCleanup(repo.cleanup)
        hits = [p for p in analyze(repo.config(), "sim.gd", Surfaces(repo.root)).problems if p.rule == "boundary"]
        self.assertEqual([p.line for p in hits], [5, 7, 8, 12])


class ParseErrorTest(unittest.TestCase):
    def test_unparsable_file_is_one_error(self):
        repo = Repo({"broken.gd": "func f(:\n\tpass\n"})
        self.addCleanup(repo.cleanup)
        result = analyze(repo.config(), "broken.gd", Surfaces(repo.root))
        self.assertFalse(result.parsed)
        self.assertEqual([p.rule for p in result.problems], ["parse-error"])


if __name__ == "__main__":
    unittest.main()
