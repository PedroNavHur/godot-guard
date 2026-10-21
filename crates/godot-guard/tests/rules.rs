//! Port of tests/test_rules.py's analysis tests: the Resource map, boundaries, and parse errors.
//! Its comment and interface tests are in tests/comments.rs and tests/interface.rs.

mod common;

use std::os::unix::fs::symlink;

use common::Repo;
use godot_guard::config::Config;
use godot_guard::rules::{self, FileResult, Surfaces};
use godot_guard::scripts::Scripts;

const WIDE_NODE: &str = include_str!("fixtures/wide_node.gd");
const RESOURCE_BASE: &str = include_str!("fixtures/resource_base.gd");
const RESOURCE_CHILD: &str = include_str!("fixtures/resource_child.gd");
const RESOURCE_GRANDCHILD: &str = include_str!("fixtures/resource_grandchild.gd");
const BOUNDARY: &str = include_str!("fixtures/boundary.gd");

fn analyze(config: &Config, path: &str, surfaces: &mut Surfaces, scripts: &Scripts) -> FileResult {
    let mut results = rules::analyze(config, &[path.to_owned()], surfaces, scripts).unwrap();
    results.remove(0)
}

fn analyze_alone(repo: &Repo, config_path: &str) -> FileResult {
    let (config, scripts) = (repo.config(), Scripts::new());
    let mut surfaces = Surfaces::new(&scripts, &repo.root, None);
    analyze(&config, config_path, &mut surfaces, &scripts)
}

fn boundary_lines(result: &FileResult) -> Vec<usize> {
    result
        .problems
        .iter()
        .filter(|p| p.rule == "boundary")
        .map(|p| p.line)
        .collect()
}

#[test]
fn resource_chain_through_two_class_names_is_excluded() {
    let repo = Repo::new(
        &[
            ("data/item.gd", RESOURCE_BASE),
            ("data/weapon.gd", RESOURCE_CHILD),
            ("data/sword.gd", RESOURCE_GRANDCHILD),
            ("ui/wide.gd", WIDE_NODE),
        ],
        None,
    );
    let (config, scripts) = (repo.config(), Scripts::new());
    let mut surfaces = Surfaces::new(&scripts, &repo.root, None);
    let sword = analyze(&config, "data/sword.gd", &mut surfaces, &scripts);
    let wide = analyze(&config, "ui/wide.gd", &mut surfaces, &scripts);
    assert!(!sword.items.iter().any(|i| i.rule == "interface-cap"));
    let caps: Vec<usize> = wide
        .items
        .iter()
        .filter(|i| i.rule == "interface-cap")
        .map(|i| i.value)
        .collect();
    assert_eq!(caps, [11]);
}

#[test]
fn overlay_wins_by_path_and_class_and_dangling_links_are_skipped() {
    let resource = "class_name Base\nextends Resource\n";
    let working = Repo::new(
        &[
            ("base.gd", resource),
            ("z_old.gd", resource),
            ("other.gd", "extends Node\n"),
        ],
        None,
    );
    let staged = Repo::new(&[("base.gd", "class_name Base\nextends Node\n")], None);
    symlink("missing.txt", working.root.join("dangling.gd")).unwrap();
    let scripts = Scripts::new();
    let listed = ["base.gd", "dangling.gd", "other.gd", "z_old.gd"]
        .map(String::from)
        .to_vec();
    let mut surfaces = Surfaces::new(&scripts, &working.root, Some(listed));
    surfaces.overlay(&scripts, &staged.root, vec!["base.gd".to_owned()]);
    let mut paths: Vec<&String> = surfaces.by_path.keys().collect();
    paths.sort();
    assert_eq!(paths, ["base.gd", "other.gd", "z_old.gd"]);
    assert_eq!(
        (
            surfaces.by_path["base.gd"].extends.as_str(),
            surfaces.by_class["Base"].extends.as_str()
        ),
        ("Node", "Node")
    );
}

#[test]
fn forbidden_patterns_in_matching_paths() {
    let config_text = concat!(
        "boundaries:\n",
        "  - name: simulation-domain\n",
        "    paths: [scripts/simulation/**]\n",
        "    forbid: ['\\bSprite3D\\b', 'res://scenes/']\n",
        "    message: simulation code must not reference presentation\n",
    );
    let repo = Repo::new(
        &[
            ("scripts/simulation/sim.gd", BOUNDARY),
            ("scripts/ui/view.gd", BOUNDARY),
        ],
        Some(config_text),
    );
    let (config, scripts) = (repo.config(), Scripts::new());
    let mut surfaces = Surfaces::new(&scripts, &repo.root, None);
    let sim = analyze(
        &config,
        "scripts/simulation/sim.gd",
        &mut surfaces,
        &scripts,
    );
    assert_eq!(boundary_lines(&sim), [4, 5]);
    let first = sim.problems.iter().find(|p| p.rule == "boundary").unwrap();
    assert!(
        first
            .message
            .contains("simulation code must not reference presentation")
    );
    let other = analyze(&config, "scripts/ui/view.gd", &mut surfaces, &scripts);
    assert!(boundary_lines(&other).is_empty());
}

#[test]
fn hash_inside_a_string_is_not_a_comment() {
    let source = concat!(
        "extends Node\n\n\nfunc f():\n\tvar a = get_node(\"#\") as Sprite3D\n",
        "\tvar b = '#' # Sprite3D in a comment\n\tvar c = get_node(&\"x#\") as Sprite3D\n",
        "\tvar d = \"\\\"#\" + str(Sprite3D)\n\tvar e = \"\"\"\n# text\n\"\"\"  # Sprite3D\n\treturn Sprite3D\n",
    );
    let config_text =
        "boundaries:\n  - name: domain\n    paths: ['**']\n    forbid: ['\\bSprite3D\\b']\n";
    let repo = Repo::new(&[("sim.gd", source)], Some(config_text));
    assert_eq!(
        boundary_lines(&analyze_alone(&repo, "sim.gd")),
        [5, 7, 8, 12]
    );
}

#[test]
fn unparsable_file_is_one_error() {
    let repo = Repo::new(&[("broken.gd", "func f(:\n\tpass\n")], None);
    let result = analyze_alone(&repo, "broken.gd");
    assert!(!result.parsed);
    assert_eq!(
        result.problems.iter().map(|p| p.rule).collect::<Vec<_>>(),
        ["parse-error"]
    );
}
